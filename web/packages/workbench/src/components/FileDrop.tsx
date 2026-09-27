/*
 * Opening local files: by dropping them, or through the picker.
 *
 * Both routes are always offered. A drop target alone is invisible to a keyboard and to
 * anyone who does not know it is there; a picker alone makes a folder of scenario files a
 * dozen clicks. The picker is a real `<button>`, so the component is operable without a
 * pointer.
 */

import { FolderOpen } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type { ChangeEvent, DragEvent, ReactNode } from "react";

import { Button, cn } from "@vitavision/ui";

import { carriesFiles, collectDroppedFiles, partitionFiles } from "./dropFiles";

/** Props of `FileDrop`. */
export interface FileDropProps {
  /** Called with the accepted files of a drop or a pick (never with an empty list). */
  onFiles: (files: File[]) => void;
  /** Called with the files a drop or a pick brought that `accept` turned away. */
  onReject?: ((files: File[]) => void) | undefined;
  /**
   * Which files to take, as `<input accept>`: extensions (`.json`), MIME types and MIME
   * wildcards (`image/*`), comma-separated. Applied to drops as well as the picker. Default: all.
   */
  accept?: string | undefined;
  /** Take several files at once. Defaults to true; when false only the first accepted file is passed. */
  multiple?: boolean | undefined;
  /**
   * The picker chooses a folder (`webkitdirectory`) instead of files. Dropped folders are
   * walked either way; files found in a folder carry `webkitRelativePath`.
   */
  directory?: boolean | undefined;
  /**
   * Take drops anywhere in the window, showing a full-window overlay while files are dragged
   * over it; the component itself then renders only the picker button (for a header). Without
   * it, the component is an inline drop zone.
   */
  overlay?: boolean | undefined;
  /** Refuses drops and blocks the picker. */
  disabled?: boolean | undefined;
  /** The picker button's label. Defaults to "Open files…" (or "Open folder…" with `directory`). */
  buttonLabel?: string | undefined;
  /** The overlay's message. Defaults to "Drop to open". */
  overlayMessage?: ReactNode;
  /** The inline zone's message, before the button. Defaults to "Drop files here, or". */
  children?: ReactNode;
  /** Merged with the zone's (or, with `overlay`, the button's) own classes through `cn`. */
  className?: string | undefined;
}

/**
 * A drop target plus an "Open files…" button over a hidden `<input type="file">`.
 *
 * Inline (the default) it is a dashed zone with a message and the button, carrying
 * `data-dragging` while files are dragged over it. With `overlay` it renders only the button
 * and takes drops anywhere in the window, showing a full-window overlay (`aria-hidden`: the
 * button is the accessible route) while a drag carrying files is over the window. Only drags
 * that carry files are reacted to; text and links pass through.
 *
 * Files are filtered by `accept` on both routes (the browser applies it to neither drops nor,
 * reliably, the picker); the rest go to `onReject`. A dropped folder is walked recursively.
 */
export function FileDrop({
  onFiles,
  onReject,
  accept,
  multiple = true,
  directory = false,
  overlay = false,
  disabled = false,
  buttonLabel,
  overlayMessage = "Drop to open",
  children = "Drop files here, or",
  className,
}: FileDropProps) {
  const inputRef = useRef<HTMLInputElement | null>(null);
  const depthRef = useRef(0);
  const [dragging, setDragging] = useState(false);
  const label = buttonLabel ?? (directory ? "Open folder…" : "Open files…");

  const deliver = (files: readonly File[]) => {
    const { accepted, rejected } = partitionFiles(files, accept);
    if (accepted.length > 0) onFiles(multiple ? accepted : accepted.slice(0, 1));
    if (rejected.length > 0) onReject?.(rejected);
  };
  const deliverRef = useRef(deliver);
  useEffect(() => {
    deliverRef.current = deliver;
  });

  // The window-wide target. `dragenter`/`dragleave` fire for every element crossed, so the
  // overlay's visibility is a depth count rather than the last event seen.
  useEffect(() => {
    if (!overlay || disabled) return;
    const onEnter = (event: globalThis.DragEvent) => {
      if (!carriesFiles(event.dataTransfer)) return;
      depthRef.current += 1;
      setDragging(true);
    };
    const onLeave = (event: globalThis.DragEvent) => {
      if (!carriesFiles(event.dataTransfer)) return;
      depthRef.current = Math.max(0, depthRef.current - 1);
      if (depthRef.current === 0) setDragging(false);
    };
    const onOver = (event: globalThis.DragEvent) => {
      if (!carriesFiles(event.dataTransfer)) return;
      event.preventDefault();
      if (event.dataTransfer) event.dataTransfer.dropEffect = "copy";
    };
    const onDrop = (event: globalThis.DragEvent) => {
      if (!event.dataTransfer || !carriesFiles(event.dataTransfer)) return;
      event.preventDefault();
      depthRef.current = 0;
      setDragging(false);
      void collectDroppedFiles(event.dataTransfer).then((files) => deliverRef.current(files));
    };
    window.addEventListener("dragenter", onEnter);
    window.addEventListener("dragleave", onLeave);
    window.addEventListener("dragover", onOver);
    window.addEventListener("drop", onDrop);
    return () => {
      window.removeEventListener("dragenter", onEnter);
      window.removeEventListener("dragleave", onLeave);
      window.removeEventListener("dragover", onOver);
      window.removeEventListener("drop", onDrop);
      depthRef.current = 0;
    };
  }, [overlay, disabled]);

  const onPick = (event: ChangeEvent<HTMLInputElement>) => {
    const files = Array.from(event.currentTarget.files ?? []);
    // Cleared so that choosing the same file again is still a change.
    event.currentTarget.value = "";
    if (files.length > 0) deliver(files);
  };

  const picker = (
    <>
      <input
        ref={(element) => {
          inputRef.current = element;
          if (element) element.webkitdirectory = directory;
        }}
        type="file"
        hidden
        tabIndex={-1}
        accept={accept}
        multiple={multiple || directory}
        disabled={disabled}
        onChange={onPick}
      />
      <Button
        icon={<FolderOpen />}
        disabled={disabled}
        onClick={() => inputRef.current?.click()}
        className={overlay ? className : undefined}
      >
        {label}
      </Button>
    </>
  );

  if (overlay) {
    return (
      <>
        {picker}
        {dragging && (
          <div
            aria-hidden
            data-dragging=""
            className="pointer-events-none fixed inset-0 z-50 grid place-items-center bg-ground/80 p-8 backdrop-blur-sm"
          >
            <div className="rounded-panel border-2 border-dashed border-signal bg-surface px-10 py-8 text-sm font-medium text-fg shadow-lg">
              {overlayMessage}
            </div>
          </div>
        )}
      </>
    );
  }

  const zoneDrag = (event: DragEvent<HTMLDivElement>, change: 1 | -1 | 0) => {
    if (disabled || !carriesFiles(event.dataTransfer)) return;
    event.preventDefault();
    if (change === 0) {
      event.dataTransfer.dropEffect = "copy";
      return;
    }
    depthRef.current = Math.max(0, depthRef.current + change);
    setDragging(depthRef.current > 0);
  };

  return (
    <div
      data-dragging={dragging ? "" : undefined}
      data-disabled={disabled ? "" : undefined}
      onDragEnter={(event) => zoneDrag(event, 1)}
      onDragLeave={(event) => zoneDrag(event, -1)}
      onDragOver={(event) => zoneDrag(event, 0)}
      onDrop={(event) => {
        if (disabled || !carriesFiles(event.dataTransfer)) return;
        event.preventDefault();
        depthRef.current = 0;
        setDragging(false);
        void collectDroppedFiles(event.dataTransfer).then(deliver);
      }}
      className={cn(
        "flex flex-col items-center justify-center gap-3 rounded-panel border-2 border-dashed border-line-strong px-6 py-8 text-center text-sm text-fg-muted transition-colors",
        "data-dragging:border-signal data-dragging:bg-signal/8 data-dragging:text-fg",
        "data-disabled:opacity-50",
        className,
      )}
    >
      <p>{children}</p>
      {picker}
    </div>
  );
}
