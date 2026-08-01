import { useEffect, useId } from "react";

export default function ConfirmDialog({
    open,
    title,
    message,
    confirmLabel,
    cancelLabel,
    busy = false,
    onConfirm,
    onCancel,
}: {
    open: boolean;
    title: string;
    message: string;
    confirmLabel: string;
    cancelLabel: string;
    busy?: boolean;
    onConfirm: () => void;
    onCancel: () => void;
}) {
    const titleId = useId();

    useEffect(() => {
        if (!open) return;
        const onKeyDown = (event: KeyboardEvent) => {
            if (event.key === "Escape" && !busy) onCancel();
        };
        window.addEventListener("keydown", onKeyDown);
        return () => window.removeEventListener("keydown", onKeyDown);
    }, [busy, onCancel, open]);

    if (!open) return null;
    return (
        <div
            className="confirm-backdrop"
            onMouseDown={(event) => {
                if (event.target === event.currentTarget && !busy) onCancel();
            }}
        >
            <div
                className="confirm-dialog"
                role="dialog"
                aria-modal="true"
                aria-labelledby={titleId}
            >
                <div className="confirm-dialog-icon" aria-hidden>
                    <svg viewBox="0 0 24 24">
                        <path d="M12 3.5 21 20H3L12 3.5Z" />
                        <path d="M12 9v5" />
                        <path d="M12 17.2v.1" />
                    </svg>
                </div>
                <div className="confirm-dialog-copy">
                    <div className="confirm-dialog-title" id={titleId}>
                        {title}
                    </div>
                    <p>{message}</p>
                </div>
                <div className="confirm-dialog-actions">
                    <button
                        className="btn confirm-cancel"
                        onClick={onCancel}
                        disabled={busy}
                        autoFocus
                    >
                        {cancelLabel}
                    </button>
                    <button
                        className="btn danger confirm-submit"
                        onClick={onConfirm}
                        disabled={busy}
                    >
                        {confirmLabel}
                    </button>
                </div>
            </div>
        </div>
    );
}
