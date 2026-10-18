import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { UiProvider } from "../ui/UiProvider.js";
import { AdminApp } from "./AdminApp.js";
import "@mantine/core/styles.css";
import "../ui/foundation.css";
import "../ui/web.css";
import "../admin.css";

document.documentElement.dataset.uiRenderer = "admin";

const root = document.getElementById("root");
if (!root) throw new Error("Admin root element is missing");
createRoot(root).render(<StrictMode><UiProvider><AdminApp /></UiProvider></StrictMode>);
