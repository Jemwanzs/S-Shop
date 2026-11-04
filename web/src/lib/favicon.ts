/** Roadmap 161: turns the image someone chose into a website favicon set, in the browser. Only these PNGs reach the server, so an SVG,
 * with whatever it may hold, is drawn by the browser as a picture (scripts never run in an image) and is never stored or served. */

export const FAVICON_SIZES = [16, 32, 48, 180, 192, 512] as const;
export type FaviconSize = (typeof FAVICON_SIZES)[number];
const ACCEPTED = /\.(png|ico|svg|jpe?g|webp)$/i;
const MAX_BYTES = 2 * 1024 * 1024;

export interface FaviconSet {
  name: string;
  images: Record<FaviconSize, Blob>;
  /** What was noticed about the source, for the person to judge. */
  notes: string[];
}

function load(file: File): Promise<HTMLImageElement> {
  const url = URL.createObjectURL(file);
  return new Promise((resolve, reject) => {
    const img = new Image();
    img.onload = () => resolve(img);
    img.onerror = () => { URL.revokeObjectURL(url); reject(new Error("This image could not be read. Try a PNG.")); };
    img.src = url;
  });
}

/** Square, centred without cropping. The Apple icon gets a white background because iPhones show transparency as black. */
function draw(img: HTMLImageElement, size: number): Promise<Blob> {
  const canvas = document.createElement("canvas");
  canvas.width = size;
  canvas.height = size;
  const g = canvas.getContext("2d");
  if (!g) return Promise.reject(new Error("This browser could not prepare the image"));
  g.imageSmoothingEnabled = true;
  g.imageSmoothingQuality = "high";
  if (size === 180) {
    g.fillStyle = "#ffffff";
    g.fillRect(0, 0, size, size);
  }
  // An SVG with no stated size reports 0 in some browsers: treat it as square.
  const w = img.naturalWidth || size;
  const h = img.naturalHeight || size;
  const scale = Math.min(size / w, size / h);
  const dw = w * scale;
  const dh = h * scale;
  g.drawImage(img, (size - dw) / 2, (size - dh) / 2, dw, dh);
  return new Promise((resolve, reject) => canvas.toBlob((b) => (b ? resolve(b) : reject(new Error("This image could not be prepared. Try a PNG."))), "image/png"));
}

export async function makeFaviconSet(file: File): Promise<FaviconSet> {
  if (!ACCEPTED.test(file.name)) throw new Error("Use a PNG, ICO or SVG image (JPEG and WebP also work)");
  if (file.size > MAX_BYTES) throw new Error("The image is over 2 MB. Use a smaller one.");
  const img = await load(file);
  const notes: string[] = [];
  const isVector = /\.svg$/i.test(file.name);
  if (img.naturalWidth && img.naturalHeight && img.naturalWidth !== img.naturalHeight) notes.push("This image is not square, so it will have empty space at the sides.");
  else if (!isVector && img.naturalWidth > 0 && img.naturalWidth < 192) notes.push("This image is small, so it may look blurred on phones. 512 × 512 is best.");
  const images = {} as Record<FaviconSize, Blob>;
  for (const size of FAVICON_SIZES) images[size] = await draw(img, size);
  URL.revokeObjectURL(img.src);
  return { name: file.name.slice(0, 120), images, notes };
}

export function faviconForm(set: FaviconSet): FormData {
  const fd = new FormData();
  fd.append("name", set.name);
  for (const size of FAVICON_SIZES) fd.append(`s${size}`, set.images[size], `${size}.png`);
  return fd;
}
