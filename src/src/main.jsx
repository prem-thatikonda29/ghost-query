import React from "react";
import ReactDOM from "react-dom/client";
import * as Tooltip from "@radix-ui/react-tooltip";
import App from "./App.tsx";
import "./index.css";
import { isMac } from "./lib/utils";

// macOS draws a frosted background behind the transparent window (see index.css).
if (isMac) document.documentElement.classList.add("translucent");

ReactDOM.createRoot(document.getElementById("root")).render(
  <React.StrictMode>
    <Tooltip.Provider delayDuration={400} skipDelayDuration={300}>
      <App />
    </Tooltip.Provider>
  </React.StrictMode>
);
