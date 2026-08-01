// 主题应用：settings.theme ∈ system / dark / light。
// system 时跟随 prefers-color-scheme，并监听系统变化。

const media = window.matchMedia("(prefers-color-scheme: dark)");
let current = "system";

function resolved(): "dark" | "light" {
    if (current === "dark" || current === "light") return current;
    return media.matches ? "dark" : "light";
}

function apply() {
    document.documentElement.dataset.theme = resolved();
}

media.addEventListener("change", () => {
    if (current === "system") apply();
});

/** 应用主题设置，后续系统主题变化自动跟随（仅 system 模式）。 */
export function applyTheme(setting: string) {
    current = setting;
    apply();
}
