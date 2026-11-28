# Receipt printing & receipt customisation — assessment (roadmap 202)

Brief: *Receipt Printing Infrastructure & Receipt Customization*. The brief asks for findings and a plan **before** any
paid service, new dependency or architectural change. This page is that assessment; nothing in it is built yet.
Status: **awaiting the owner's approval** (see the decisions at the end).

## 1. What S'Shop has today (inspected in the code, 2026-11-28)

| Area | State | Where |
|---|---|---|
| One receipt template for every channel | ✅ Implemented — frozen snapshot per receipt (original, updated after a return, corrected after Edit Sale) | `server/src/routes/receipts.rs`, `web/src/lib/receipt.ts`, `web/src/components/Receipt.tsx` |
| Print | ◐ Browser printing only: a 50 mm-wide PDF is generated (jsPDF) and opened in a hidden frame that calls the browser's print dialog — **the dialog opens every time** | `printReceipt()` in `web/src/lib/receipt.ts` |
| PDF / Image / Email / WhatsApp / secure link | ✅ Implemented (PDF via jsPDF, PNG via html2canvas, email with the PDF attached, WhatsApp dialog of roadmap 201, public `/r/{token}` link that renders the snapshot) | same files, `ReceiptPanel.tsx`, `WhatsAppShare.tsx` |
| Paper sizes | ◐ One fixed 50 mm content width. It prints on 58 mm and 80 mm rolls (inside their printable width) but does not use 80 mm's width, and there is no A4 layout or paper-size setting | `W = 50` in `receipt.ts` |
| Thermal printers (ESC/POS) | ❌ Not implemented for receipts. Printers work only through their installed OS driver and the browser dialog. (Barcode **labels** already export ZPL / TSPL files — roadmap 138 — a precedent for printer-language generators written in-house.) | `LabelsDialog.tsx` |
| Automatic paper cut | ◐ Only if the printer's driver is set to cut at the end of a job — S'Shop sends no cut command | — |
| Direct (dialog-free) printing | ❌ | — |
| Automatic printing after a sale | ❌ | — |
| Printer registry, default printer per branch/device, test print, print history, retry | ❌ | — |
| Shared / remote printing across branches | ❌ | — |
| Receipt appearance settings | ◐ Settings → Sales: logo, branch, contact, customer, salesperson, loyalty, payment reference on/off; font *sans* or *thermal*; footer text | `ReceiptSettings` in `server/src/settings.rs` |
| Product codes, discounts, outstanding credit balance on the receipt | ◐ Discounts and payments are shown; product codes and the remaining credit balance are not configurable lines | `receipt.ts` |
| Product Name Appearance on receipts (show / blur / mask / hide) | ❌ | — |
| Per-receipt appearance override | ❌ | — |
| Credit repayment receipts | ❌ No separate document; roadmap 201 sends a payment confirmation linking the sale's receipt | `CreditDetail.tsx` |
| Permissions | ◐ One permission, *Print & share receipts* (`sales.print`) | `server/src/perms.rs` |
| Statutory content | S'Shop records no tax (no VAT / eTIMS integration); business name, receipt number, date and totals are always printed and are not switchable | — |

## 2. What browsers can and cannot do (the hardware reality)

- **Any printer with an OS driver** (USB, Bluetooth, Wi-Fi, Ethernet) prints through the browser's print dialog on Windows,
  macOS, ChromeOS and Android (Mopria / the maker's print service). iPhone/iPad print only to **AirPrint** printers.
- **Silent printing** (no dialog) is not possible from an ordinary web page. On a dedicated till computer, Chrome or Edge
  started with `--kiosk-printing` prints straight to the default printer — a free, documented setup per device.
- **ESC/POS straight from the browser** (no driver, with the cut command):
  - **WebUSB / Web Serial**: Chrome and Edge on Windows, macOS, ChromeOS and Linux; WebUSB on Android Chrome. On Windows a
    USB printer that already has a vendor driver may need its driver swapped (WinUSB) before WebUSB can claim it, which
    breaks other software using that driver. Serial and USB-serial printers are easier.
  - **Web Bluetooth**: only Bluetooth **Low Energy** printers, Chrome on Android, Windows and macOS. Many cheap 58 mm
    portable printers are Bluetooth Classic (SPP) only, which no browser can reach.
  - **Safari / iOS**: none of these APIs exist.
  - **Network printers** (Ethernet / Wi-Fi, raw port 9100): a browser cannot open raw sockets. Reaching them directly
    needs a local agent.
- **Remote printing** (print at another branch) and **print confirmation** (the printer really printed) need something
  running next to the printer: a local agent or a cloud print service.

## 3. Options compared

| Option | What it gives | Cost | New dependency / architecture |
|---|---|---|---|
| A. Browser printing, done properly | 58 / 80 mm / A4 layouts; preview; auto-print after a sale (dialog, or silent with kiosk printing); driver-level auto-cut; print log | None | None |
| B. In-browser ESC/POS (WebUSB / Web Serial / Web Bluetooth) | Dialog-free printing and cutting from Chrome/Edge desktop and Android to USB, serial and BLE printers | None | No new library: the ESC/POS encoder is written in-house (like ZPL/TSPL). Browser APIs only |
| C. S'Shop print agent (Rust) | Network (9100) and USB printers without a dialog on every OS; shared branch printers; remote printing across branches; queued jobs with confirmation, retry and history | No subscription. Optional Windows/macOS code-signing certificate (yearly) so installers are trusted | Yes: a small Rust program installed on one computer per branch, connecting out to S'Shop (no inbound ports); a print-job queue in PostgreSQL |
| QZ Tray | Like C, ready-made (Java app) | Free edition shows trust prompts; silent use needs a paid certificate/licence (check current pricing) | Java + their signing |
| PrintNode | Cloud remote printing with a desktop client | Monthly subscription per account/printers (check current pricing) | Third-party cloud receives receipt content |

**Recommendation:** A, then B, then C. That is S'Shop-owned and costs nothing recurring; QZ Tray and PrintNode are not
needed. C replaces them and keeps receipt data within S'Shop. Each phase is useful on its own.

## 4. Printers named in the brief

All are ESC/POS with an auto-cutter. Check the exact model variant's interfaces before buying.

| Printer | Paper | Expected path in S'Shop |
|---|---|---|
| Xprinter XP-Q80B | 80 mm | A (driver), B (USB; WebUSB may need the WinUSB driver on Windows), C (USB / LAN variant) |
| Xprinter XP-Q890K | 80 mm | A, B (USB), C (USB / LAN / Wi-Fi variants) |
| Epson TM-T20III | 80 mm | A (Epson driver, auto-cut in the driver), B (USB / serial variant), C (USB / Ethernet variant) |
| Portable Bluetooth thermal | 58 mm | A on Android (maker's print service); B only if the printer supports **BLE**; Classic-only printers need the maker's app or C on a PC |
| Other ESC/POS printers | 58 / 80 mm | Same as above |

## 5. Receipt customisation (Product Name Appearance etc.)

To meet the brief's security rule (*concealed names cannot be recovered from the shared receipt, the PDF text or the
styling*), concealment has to happen **on the server**. The receipt data the browser, the PDF, the image or the public
link receive must already carry no product name (show / blur / mask / hide). Otherwise the name stays in the page or
the JSON.

- **Mask:** a solid bar of fixed width.
- **Blur:** an image of meaningless glyphs.
- **Hide:** the line is kept without the name.

The original names stay in the sale, the ledger, reports and the audit trail. The chosen appearance is stored with each
receipt (and with each shared link). A per-receipt override issues a new rendering of the same receipt; the sale itself
never changes.

New lines to add: product codes, the outstanding credit balance, and payment breakdowns. These are switchable alongside
the existing toggles, and statutory lines are never switchable.

## 6. Proposed phases

| Phase | Scope | Effort (relative to recent roadmap items) | Hardware testing |
|---|---|---|---|
| 202-A | **Settings → Sales & Payments → Receipt & Printer Configuration**: paper size 58 / 80 mm / A4 (80 mm default) per branch and device; preview; test print; auto-print after a sale (off by default); kiosk-printing guide; print log (who, receipt, branch, device, when — a browser can only confirm the job was sent). **Receipt appearance**: the new lines and **Product Name Appearance** (server-side concealment across preview, print, PDF, image, email, WhatsApp, link), per-receipt override. New permissions under **Sales & Receipts**: print, reprint, share, customise one receipt, manage receipt appearance, manage printers, print remotely, view print history (existing roles keep what *Print & share receipts* gave them) | ~2 items (like 196) | One 80 mm and one 58 mm printer through their drivers; Windows, macOS, Android, iPhone |
| 202-B | Printers registry per branch and device; in-browser ESC/POS (WebUSB, Web Serial, Web Bluetooth BLE) with cut and drawer kick; default printer per device; dialog-free print and auto-print; failures recorded, retry | ~1–2 items | The printers above over USB, serial and BLE; Chrome/Edge on Windows, macOS and Android |
| 202-C | S'Shop print agent (Rust): install, pair with a branch, network/USB printers, shared branch printers, remote printing to authorised branches, job queue with confirmation, history and retry; audit of remote prints | ~3 items | A LAN printer and a USB printer on a Windows PC and a Mac; remote print between two branches |

## 7. Security & integrity (all phases)

- Printing, reprinting and sharing only read the frozen receipt — they never create sales, payments, stock movements or
  loyalty points. A failed print never touches a completed sale.
- Remote printing only to printers registered to branches the person may access, with the new permission. Printer
  agents authenticate with a per-device token that can be revoked. Every remote print and every configuration change is
  audited.
- Customisation changes the receipt's rendering only, never the sale. Statutory lines (business, receipt number, date,
  totals) cannot be switched off.
- Tenant isolation is unchanged: printers, jobs and agents belong to one business.

## 8. Decisions for the owner

1. Approve phase **202-A** (no new dependency, no cost)?
2. Approve **202-B** (browser printer APIs, no new library)?
3. Approve **202-C** (an installable S'Shop print agent), and whether to buy code-signing certificates for it?
4. Which printers will be used for hardware testing (one 80 mm, one 58 mm portable)?
