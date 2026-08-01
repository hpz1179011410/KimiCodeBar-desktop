// i18n 初始化：语言设置存 settings.language（system / zh / en），切换即时生效。
import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import zh from "./zh.json";
import en from "./en.json";

export function resolveLanguage(setting: string): "zh" | "en" {
    if (setting === "zh" || setting === "en") return setting;
    return navigator.language.toLowerCase().startsWith("zh") ? "zh" : "en";
}

// 模块加载时同步完成初始化（内联 resources，init 同步就绪）——
// 必须在任何组件调用 useTranslation() 之前完成，否则首帧渲染直接抛错白屏。
void i18n.use(initReactI18next).init({
    resources: {
        zh: { translation: zh },
        en: { translation: en },
    },
    lng: resolveLanguage("system"),
    fallbackLng: "en",
    interpolation: { escapeValue: false },
});

/** 应用语言设置（i18next 已在模块加载时初始化）。 */
export function applyLanguage(setting: string) {
    void i18n.changeLanguage(resolveLanguage(setting));
}

export default i18n;
