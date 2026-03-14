import React from "react";
import ReactDOM from "react-dom/client";

import { AppRouterProvider } from "./app/router";
import { ThemeProvider } from "./app/theme-provider";
import "./index.css";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <ThemeProvider>
      <AppRouterProvider />
    </ThemeProvider>
  </React.StrictMode>,
);
