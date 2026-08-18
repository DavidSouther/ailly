export function syncThemeWithSystem(): void {
  const query = window.matchMedia("(prefers-color-scheme: dark)");

  const apply = (matches: boolean) => {
    document.documentElement.setAttribute("data-theme", matches ? "dark" : "light");
  };

  apply(query.matches);
  query.addEventListener("change", (event) => apply(event.matches));
}
