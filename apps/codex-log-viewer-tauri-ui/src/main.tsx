import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import App from "./App.tsx";
import { DebugPanel } from "./components/debug-panel";
import { ExternalLinkGuard } from "./components/external-link-guard";
import "./index.css";

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <ExternalLinkGuard />
    {import.meta.env.DEV ? <DebugPanel /> : null}
    <App />
  </StrictMode>,
);
