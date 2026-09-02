// ----------
// React Application Mount Point
// Description: Mounts the root React component tree into the DOM shell and loads global CSS.
// ----------

import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { ThemeProvider } from "./components/theme-provider";
import "./index.css";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <ThemeProvider defaultTheme="system" storageKey="clipbox-theme">
      <App />
    </ThemeProvider>
  </React.StrictMode>
);
