import { createContext, useContext } from "react";

import type { RouteId } from "./routes";

export const NavigationContext = createContext<(route: RouteId) => void>(() => {});

export function useNavigate() {
  return useContext(NavigationContext);
}
