/** Hash router: #/ (home), #/settings, #/summary, #/history, #/floating, #/marker. */
export type Route = "/" | "/settings" | "/summary" | "/history" | "/floating" | "/marker";

const KNOWN: Route[] = ["/", "/settings", "/summary", "/history", "/floating", "/marker"];

export function parseHash(hash: string): Route {
  const path = hash.replace(/^#/, "").split("?")[0] || "/";
  const normalised = path.startsWith("/") ? path : `/${path}`;
  return (KNOWN as string[]).includes(normalised) ? (normalised as Route) : "/";
}

let route = $state<Route>(typeof location !== "undefined" ? parseHash(location.hash) : "/");
let bound = false;

export function getRoute(): Route {
  return route;
}

export function navigate(to: Route): void {
  if (typeof location !== "undefined") {
    const target = to === "/" ? "#/" : `#${to}`;
    if (location.hash !== target) {
      location.hash = target;
      return; // hashchange updates `route`
    }
  }
  route = to;
}

export function bindRouter(): () => void {
  if (bound || typeof window === "undefined") return () => {};
  bound = true;
  const onChange = () => {
    route = parseHash(location.hash);
  };
  window.addEventListener("hashchange", onChange);
  onChange();
  return () => {
    window.removeEventListener("hashchange", onChange);
    bound = false;
  };
}
