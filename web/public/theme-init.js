// Applies the saved theme before first paint (no flash). A file, not inline, so the CSP can forbid inline scripts.
try {
  var t = localStorage.getItem("sshop.theme");
  if (t === "dark" || (!t && matchMedia("(prefers-color-scheme: dark)").matches)) document.documentElement.classList.add("dark");
} catch (e) {}
