// 设置窗口入口：渲染共享的 SettingsApp 组件（面板内嵌与独立窗口共用同一实现）。
import React from "react";
import { createRoot } from "react-dom/client";
import { SettingsApp } from "./SettingsApp";

createRoot(document.getElementById("root")!).render(
    <React.StrictMode>
        <SettingsApp />
    </React.StrictMode>,
);
