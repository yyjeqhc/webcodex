import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App.js";
import "./styles.css";
import "./styles-production.css";
import "./styles-agents.css";
import "./styles-responsive.css";

const root = document.getElementById("root");
if (!root) throw new Error("Runtime WebUI root element is missing");

createRoot(root).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
