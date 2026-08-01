import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { Toasty, TooltipProvider } from "@cloudflare/kumo";
import "./app.css";
import App from "./App";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <Toasty>
      <TooltipProvider>
        <App />
      </TooltipProvider>
    </Toasty>
  </StrictMode>,
);
