import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import { LocaleProvider } from "./i18n/locale";
import { installDesktopWindowBehavior } from "./lib/desktop-window-behavior";
import "@mantine/core/styles.css";
import "../../../frontend/src/ui/foundation.css";
import "./styles/theme.css";
import "./styles/app.css";
import "./styles/workspace.css";
import "./styles/connections-tools.css";
import "./styles/product.css";

document.documentElement.dataset.uiRenderer = "desktop";
// Development keeps browser reload and inspection affordances.
if (import.meta.env.PROD) installDesktopWindowBehavior();

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <LocaleProvider>
      <App />
    </LocaleProvider>
  </StrictMode>,
);
