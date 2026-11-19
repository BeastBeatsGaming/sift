import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { FeedbackProvider } from "./components/feedback";
import { ContextMenuProvider } from "./components/menu";
import { installErrorLogging } from "./meta";
import "./styles.css";

installErrorLogging();

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <FeedbackProvider>
      <ContextMenuProvider>
        <App />
      </ContextMenuProvider>
    </FeedbackProvider>
  </React.StrictMode>,
);
