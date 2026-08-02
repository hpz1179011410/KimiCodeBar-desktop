import { useEffect, useId, useState } from "react";
import { useTranslation } from "react-i18next";
import * as ipc from "../lib/ipc";
import type { AppUpdateInfo, AppUpdateMetadata } from "../types";

type UpdatePhase = "idle" | "loading" | "offer" | "downloading" | "ready" | "installing" | "error";

function formatBytes(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

function cleanReleaseNotes(notes: string): string {
    return notes
        .replace(/^#{1,6}\s+/gm, "")
        .replace(/\*\*([^*]+)\*\*/g, "$1")
        .replace(/\[([^\]]+)]\([^)]+\)/g, "$1")
        .trim();
}

export default function AppUpdateDialog({
    open,
    info,
    onClose,
}: {
    open: boolean;
    info: AppUpdateInfo | null;
    onClose: () => void;
}) {
    const { t } = useTranslation();
    const titleId = useId();
    const [phase, setPhase] = useState<UpdatePhase>("idle");
    const [metadata, setMetadata] = useState<AppUpdateMetadata | null>(null);
    const [downloaded, setDownloaded] = useState(0);
    const [contentLength, setContentLength] = useState<number | null>(null);
    const [noticeOpen, setNoticeOpen] = useState(false);
    const [error, setError] = useState("");

    useEffect(() => {
        if (!open || phase !== "idle") return;
        let disposed = false;
        setPhase("loading");
        setError("");
        void ipc
            .prepareAppUpdate()
            .then((update) => {
                if (disposed) return;
                if (!update) {
                    setError(t("update.noLongerAvailable"));
                    setPhase("error");
                    return;
                }
                setMetadata(update);
                setPhase("offer");
            })
            .catch((reason) => {
                if (disposed) return;
                setError(String(reason));
                setPhase("error");
            });
        return () => {
            disposed = true;
        };
    }, [open, phase, t]);

    useEffect(() => {
        if (!open && phase === "offer") {
            setPhase("idle");
        }
    }, [open, phase]);

    const closeOffer = () => {
        setNoticeOpen(false);
        setPhase("idle");
        setMetadata(null);
        void ipc.discardAppUpdate().catch(() => undefined);
        onClose();
    };

    const startDownload = async () => {
        setDownloaded(0);
        setContentLength(null);
        setError("");
        setPhase("downloading");
        setNoticeOpen(false);
        onClose();
        try {
            await ipc.downloadAppUpdate((event) => {
                if (event.event === "started") {
                    setContentLength(event.data.content_length);
                } else if (event.event === "progress") {
                    setDownloaded((value) => value + event.data.chunk_length);
                }
            });
            setPhase("ready");
            setNoticeOpen(true);
        } catch (reason) {
            setError(String(reason));
            setPhase("error");
            setNoticeOpen(true);
        }
    };

    const install = async () => {
        setPhase("installing");
        setError("");
        try {
            await ipc.installAppUpdate();
            setNoticeOpen(false);
            onClose();
        } catch (reason) {
            setError(String(reason));
            setPhase("error");
            setNoticeOpen(true);
        }
    };

    const showDialog = open || noticeOpen;
    const percent = contentLength
        ? Math.min(100, Math.round((downloaded / contentLength) * 100))
        : null;
    const latest = metadata?.latest ?? info?.latest ?? "—";

    return (
        <>
            {phase === "downloading" && (
                <div
                    className="update-download-toast"
                    role="status"
                    aria-label={t("update.downloading")}
                >
                    <span className="update-download-toast-icon" aria-hidden>
                        ↓
                    </span>
                    <span className="update-download-toast-copy">
                        <strong>{t("update.downloadingVersion", { version: latest })}</strong>
                        <span>
                            {contentLength
                                ? `${formatBytes(downloaded)} / ${formatBytes(contentLength)}`
                                : formatBytes(downloaded)}
                        </span>
                    </span>
                    <span className="update-download-toast-percent">
                        {percent == null ? "…" : `${percent}%`}
                    </span>
                    <span className="update-download-toast-track" aria-hidden>
                        <span style={{ width: `${percent ?? 28}%` }} />
                    </span>
                </div>
            )}

            {showDialog && phase !== "idle" && phase !== "downloading" && (
                <div className="confirm-backdrop update-backdrop">
                    <div
                        className="update-dialog"
                        role="dialog"
                        aria-modal="true"
                        aria-labelledby={titleId}
                    >
                        <div className="update-dialog-head">
                            <div className="update-dialog-icon" aria-hidden>
                                {phase === "ready" ? "✓" : "↑"}
                            </div>
                            <div>
                                <div className="update-dialog-title" id={titleId}>
                                    {phase === "ready"
                                        ? t("update.downloadComplete")
                                        : t("update.available", { version: latest })}
                                </div>
                                <div className="update-dialog-version">
                                    {t("update.versionChange", {
                                        current: metadata?.current ?? info?.current ?? "—",
                                        latest,
                                    })}
                                </div>
                            </div>
                        </div>

                        {phase === "loading" ? (
                            <div className="update-dialog-loading">
                                <span className="spinner" />
                                {t("update.loadingNotes")}
                            </div>
                        ) : phase === "error" ? (
                            <div className="update-dialog-error" role="alert">
                                {t("update.failed", { msg: error })}
                            </div>
                        ) : phase === "ready" || phase === "installing" ? (
                            <p className="update-dialog-ready-copy">{t("update.readyToInstall")}</p>
                        ) : (
                            <>
                                <div className="update-dialog-section-title">
                                    {t("update.releaseNotes")}
                                </div>
                                <div className="update-dialog-notes">
                                    {cleanReleaseNotes(metadata?.notes ?? "")}
                                </div>
                            </>
                        )}

                        <div className="update-dialog-actions">
                            {phase === "offer" && (
                                <>
                                    <button className="btn update-secondary" onClick={closeOffer}>
                                        {t("update.notNow")}
                                    </button>
                                    <button
                                        className="btn primary update-primary"
                                        onClick={() => void startDownload()}
                                    >
                                        {t("update.downloadInBackground")}
                                    </button>
                                </>
                            )}
                            {phase === "ready" && (
                                <>
                                    <button
                                        className="btn update-secondary"
                                        onClick={() => {
                                            setNoticeOpen(false);
                                            onClose();
                                        }}
                                    >
                                        {t("update.installLater")}
                                    </button>
                                    <button
                                        className="btn primary update-primary"
                                        onClick={() => void install()}
                                    >
                                        {t("update.installNow")}
                                    </button>
                                </>
                            )}
                            {phase === "installing" && (
                                <button className="btn primary update-primary" disabled>
                                    {t("update.installing")}
                                </button>
                            )}
                            {phase === "error" && (
                                <>
                                    <button
                                        className="btn update-secondary"
                                        onClick={() => {
                                            setNoticeOpen(false);
                                            setPhase("idle");
                                            onClose();
                                        }}
                                    >
                                        {t("common.cancel")}
                                    </button>
                                    <button
                                        className="btn primary update-primary"
                                        onClick={() =>
                                            void ipc.openUrl(
                                                metadata?.release_url ?? info?.release_url ?? "",
                                            )
                                        }
                                    >
                                        {t("update.openReleasePage")}
                                    </button>
                                </>
                            )}
                        </div>
                    </div>
                </div>
            )}
        </>
    );
}
