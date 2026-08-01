import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import {
    cancelLogin,
    onLoginError,
    onLoginSuccess,
    openUrl,
    setApiKey,
    setLoginMethod,
    startDeviceLogin,
} from "../lib/ipc";
import { useNow } from "./UsageCard";

type Phase = "idle" | "waiting";

function fmtElapsed(ms: number): string {
    const s = Math.floor(ms / 1000);
    const m = Math.floor(s / 60);
    return `${String(m).padStart(2, "0")}:${String(s % 60).padStart(2, "0")}`;
}

/** 未登录时的登录覆盖层：OAuth Device Flow + API Key 两种入口。 */
export default function LoginOverlay({ onSuccess }: { onSuccess: () => void }) {
    const { t } = useTranslation();
    const [phase, setPhase] = useState<Phase>("idle");
    const [userCode, setUserCode] = useState("");
    const [verifyUrl, setVerifyUrl] = useState<string | null>(null);
    const [error, setError] = useState("");
    const [keyInput, setKeyInput] = useState("");
    const [busy, setBusy] = useState(false);
    const [copied, setCopied] = useState(false);
    const [startedAt, setStartedAt] = useState(0);
    const cancelRef = useRef(false);
    const now = useNow(phase === "waiting" ? 1000 : 60_000);

    useEffect(() => {
        let disposed = false;
        const unlisteners: Array<() => void> = [];
        void onLoginSuccess(() => {
            if (!disposed) onSuccess();
        }).then((u) => unlisteners.push(u));
        void onLoginError((msg) => {
            if (disposed) return;
            if (cancelRef.current) {
                // 用户主动取消也会触发 login-error，忽略
                cancelRef.current = false;
                return;
            }
            setError(msg);
            setPhase("idle");
        }).then((u) => unlisteners.push(u));
        return () => {
            disposed = true;
            unlisteners.forEach((u) => u());
        };
    }, [onSuccess]);

    const startOauth = async () => {
        setError("");
        setBusy(true);
        try {
            const start = await startDeviceLogin();
            setUserCode(start.user_code);
            setVerifyUrl(start.verification_uri_complete);
            setStartedAt(Date.now());
            setPhase("waiting");
            if (start.verification_uri_complete) {
                await openUrl(start.verification_uri_complete).catch(() => undefined);
            }
        } catch (e) {
            setError(String(e));
        } finally {
            setBusy(false);
        }
    };

    const cancel = async () => {
        cancelRef.current = true;
        await cancelLogin().catch(() => undefined);
        setPhase("idle");
        setUserCode("");
    };

    const copyCode = async () => {
        if (!userCode) return;
        try {
            await navigator.clipboard.writeText(userCode);
            setCopied(true);
            setTimeout(() => setCopied(false), 1500);
        } catch {
            /* 剪贴板不可用时静默忽略 */
        }
    };

    const saveKey = async () => {
        const key = keyInput.trim();
        if (!key) return;
        setError("");
        setBusy(true);
        try {
            await setApiKey(key);
            await setLoginMethod("api_key");
            onSuccess();
        } catch (e) {
            setError(String(e));
        } finally {
            setBusy(false);
        }
    };

    return (
        <div className="login-overlay">
            <div className="login-card">
                <div className="logo-badge large">K</div>
                <h2 className="login-title">{t("login.title")}</h2>
                <p className="login-subtitle">{t("login.subtitle")}</p>

                {error && <div className="banner error">{t("login.failed", { msg: error })}</div>}

                {phase === "idle" && (
                    <>
                        <button className="btn primary block" onClick={startOauth} disabled={busy}>
                            {t("login.authorize")}
                        </button>
                        <details className="login-alt">
                            <summary>{t("login.apiKeyEntry")}</summary>
                            <div className="login-apikey">
                                <input
                                    className="input"
                                    type="password"
                                    placeholder={t("login.apiKeyPlaceholder")}
                                    value={keyInput}
                                    onChange={(e) => setKeyInput(e.target.value)}
                                    onKeyDown={(e) => {
                                        if (e.key === "Enter") void saveKey();
                                    }}
                                />
                                <button
                                    className="btn primary block"
                                    onClick={saveKey}
                                    disabled={busy || !keyInput.trim()}
                                >
                                    {t("login.saveKey")}
                                </button>
                            </div>
                        </details>
                    </>
                )}

                {phase === "waiting" && (
                    <div className="login-waiting">
                        <div className="spinner" />
                        <div className="waiting-text">{t("login.waiting")}</div>
                        <div className="waiting-elapsed">
                            {t("login.elapsed", { time: fmtElapsed(now - startedAt) })}
                        </div>
                        <div className="user-code-row">
                            <div className="user-code">{userCode}</div>
                            <button className="btn sm" onClick={copyCode}>
                                {copied ? t("login.copied") : t("login.copy")}
                            </button>
                        </div>
                        <p className="code-hint">{t("login.codeHint")}</p>
                        <div className="login-actions">
                            {verifyUrl && (
                                <button
                                    className="btn ghost"
                                    onClick={() => openUrl(verifyUrl).catch(() => undefined)}
                                >
                                    {t("login.openAgain")}
                                </button>
                            )}
                            <button className="btn ghost" onClick={cancel}>
                                {t("login.cancel")}
                            </button>
                        </div>
                    </div>
                )}
            </div>
        </div>
    );
}
