import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./index.css";
import { apply, storedChoice } from "./theme";

// Before the first render, and before React exists. Nothing else applied a
// stored choice at startup: the toggle wrote `data-theme` when clicked and
// `localStorage` remembered it, but on the next launch the attribute was never
// set again — so an explicit "Dark" on a light Mac came back light, with the
// toggle still showing Dark. Found by looking at the window.
apply(storedChoice());

const root = document.getElementById("root");
if (!root) throw new Error("index.html is missing #root");

ReactDOM.createRoot(root).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
