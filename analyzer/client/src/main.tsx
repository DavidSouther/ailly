import React from "react";
import { createRoot } from "react-dom/client";

import "./styles/theme.css";
import { App } from "./App";

const root = document.getElementById("root");

if (!root) {
  throw new Error("Ailly Analyzer root element is missing");
}

createRoot(root).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
