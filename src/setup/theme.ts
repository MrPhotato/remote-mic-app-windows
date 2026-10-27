/** Installer theme is transient: never read or write the installed app's preferences. */
export function followSystemTheme(): () => void {
  const query = window.matchMedia("(prefers-color-scheme: dark)");
  const apply = () => { document.documentElement.dataset.theme = query.matches ? "dark" : "light"; };
  apply();
  query.addEventListener("change", apply);
  return () => query.removeEventListener("change", apply);
}
