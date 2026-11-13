// A draft's attachments: files staged as they are added (on the server in
// cloud mode, with progress), the attachments a forward keeps, and the size
// limit, checked before anything is staged.
import { cleanup, signal } from "nsh";
import { t } from "nsh/i18n";
import { Mail, PageDrop, type DraftAttachment, type Message, type Upload } from "nsh-mail/native";
import { fileSize } from "./format";

export interface Attached {
  key: number;
  name: string;
  size: number;
  /** Bytes staged so far. */
  sent: number;
  /** What the draft names; unset while the file is staged. */
  ref?: DraftAttachment;
  failed?: boolean;
}

/** A file to attach: a picked or pasted `File`, a local file's path or `file:` URL (desktop drops,
 * pasted file manager copies), or the bytes of a file dropped on the web page. */
export type Incoming = Blob | { kind: "path"; path: string; type: string } | { kind: "bytes"; name: string; type: string; data: Uint8Array };

let nextKey = 0;

export function createAttachments(onError: (message: string) => void) {
  const list = signal<Attached[]>([]);
  const removed = new Set<number>();
  let sent = false;

  const update = (key: number, patch: Partial<Attached>) =>
    list.set(list().map((a) => (a.key === key ? { ...a, ...patch } : a)));
  const total = () => list().reduce((n, a) => n + (a.failed ? 0 : a.size), 0);
  const tooLarge = () =>
    t("compose.tooLarge", { limit: fileSize(Mail.attachmentLimit()) });

  async function stage(file: Incoming) {
    let name: string;
    let staging: (progress: (sent: number) => void) => Promise<Upload>;
    let size = 0;
    try {
      if ("kind" in file && file.kind === "path") {
        name = file.path.split(/[\\/]/).pop() || "file";
        try {
          name = decodeURIComponent(name);
        } catch {}
        const { path, type } = file;
        staging = (progress) => Mail.stageFile(path, type || "application/octet-stream", progress);
      } else {
        let bytes: Uint8Array;
        let type: string;
        if ("kind" in file) {
          ({ name, type } = file);
          bytes = file.data;
        } else {
          name = (file as File).name || "image.png";
          type = file.type;
          if (total() + file.size > Mail.attachmentLimit()) return onError(tooLarge());
          bytes = new Uint8Array(await file.arrayBuffer());
        }
        if (total() + bytes.length > Mail.attachmentLimit()) return onError(tooLarge());
        size = bytes.length;
        const n = name;
        staging = (progress) => Mail.stage(n, type || "application/octet-stream", bytes, progress);
      }
    } catch (e) {
      return onError(t("compose.unreadable", { detail: e instanceof Error ? e.message : String(e) }));
    }
    const key = ++nextKey;
    list.set([...list(), { key, name, size, sent: 0 }]);
    try {
      const upload = await staging((n) => update(key, { sent: n }));
      if (removed.has(key) || sent) return void Mail.unstage(upload.id).catch(() => {});
      update(key, { name: upload.filename, size: upload.size, sent: upload.size, ref: { kind: "upload", id: upload.id } });
      // A local file's size is known once it is read.
      if (total() > Mail.attachmentLimit()) {
        removed.add(key);
        list.set(list().filter((x) => x.key !== key));
        void Mail.unstage(upload.id).catch(() => {});
        onError(tooLarge());
      }
    } catch (e) {
      if (removed.has(key)) return;
      const code = (e as { code?: string })?.code;
      if (code === "ERR_TOO_LARGE") {
        list.set(list().filter((x) => x.key !== key));
        return onError(tooLarge());
      }
      update(key, { failed: true });
      onError(t("compose.uploadFailed", { name, detail: e instanceof Error ? e.message : String(e) }));
    }
  }

  // Staged files the draft did not send are dropped with the composer.
  cleanup(() => {
    if (!sent) for (const a of list()) if (a.ref?.kind === "upload") void Mail.unstage(a.ref.id).catch(() => {});
  });

  return {
    items: () => list(),
    total,
    /** Some file is still being staged. */
    pending: () => list().some((a) => !a.ref && !a.failed),
    /** Over the limit, as the message would be sent. */
    overLimit: () => total() > Mail.attachmentLimit(),
    limitText: tooLarge,
    add(files: Incoming[]) {
      for (const file of files) void stage(file);
    },
    /** A forward's attachments, each one the user may remove. */
    forward(m: Message) {
      list.set([
        ...list(),
        ...m.attachments.map((a) => ({
          key: ++nextKey,
          name: a.filename || t("reader.untitledAttachment"),
          size: a.size,
          sent: a.size,
          ref: { kind: "forward" as const, messageId: m.id, index: a.index },
        })),
      ]);
    },
    remove(key: number) {
      const a = list().find((x) => x.key === key);
      removed.add(key);
      list.set(list().filter((x) => x.key !== key));
      if (a?.ref?.kind === "upload") void Mail.unstage(a.ref.id).catch(() => {});
    },
    /** What the draft sends. */
    draft: (): DraftAttachment[] => list().flatMap((a) => (a.ref ? [a.ref] : [])),
    /** The draft went out with these: nothing to drop. */
    sent() {
      sent = true;
    },
  };
}

export type Attachments = ReturnType<typeof createAttachments>;

/** Files dragged onto the page (the web; `PageDrop` does nothing elsewhere). */
export function onPageDrop(dragging: (on: boolean) => void, dropped: (files: Incoming[]) => void) {
  let stop: (() => void) | undefined;
  let gone = false;
  void PageDrop.listen(dragging, (name, mime, data) => dropped([{ kind: "bytes", name, type: mime, data }])).then((s) =>
    gone ? s() : (stop = s),
  );
  cleanup(() => {
    gone = true;
    stop?.();
  });
}
