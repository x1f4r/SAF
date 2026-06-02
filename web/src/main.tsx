import "@fontsource-variable/nunito";
import "./theme.css";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { SIDEBAR_CSS } from "./components/Sidebar";
import { TOAST_CSS } from "./components/Toast";
import { StoreProvider } from "./store";

const style = document.createElement("style");
style.textContent = SIDEBAR_CSS + "\n" + TOAST_CSS;
document.head.appendChild(style);

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <StoreProvider>
      <App />
    </StoreProvider>
  </StrictMode>
);
