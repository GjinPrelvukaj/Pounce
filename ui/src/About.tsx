import { useEffect, useRef, useState } from "react";
import { engineInfo, type CrawlHandle, type EngineInfo } from "./engine";

/// Where `engine 0.0.1 · schema 13 · 30 rules` went.
///
/// It was in the header, next to the product name, where it read as a version
/// check written for the developer — nobody opening a crawl needs a schema
/// number. It is still worth having: a mismatched frontend and engine say so
/// out loud here, and a `.pounce` file written by a different build is a real
/// thing to be able to check.
export function About({ handle }: { handle: CrawlHandle | null }) {
  const [info, setInfo] = useState<EngineInfo | null>(null);
  const dialog = useRef<HTMLDialogElement>(null);

  useEffect(() => {
    engineInfo()
      .then(setInfo)
      .catch(() => setInfo(null));
  }, []);

  return (
    <>
      <button
        onClick={() => dialog.current?.showModal()}
        aria-label="About Pounce"
        title="About Pounce"
        className="btn border-transparent bg-transparent px-2"
      >
        ⓘ
      </button>
      {/* `<dialog>` rather than a div and a portal: the browser supplies the
          backdrop, the focus trap, and Escape to close — three things a
          hand-rolled modal gets wrong. */}
      <dialog
        ref={dialog}
        onClick={(e) => e.target === dialog.current && dialog.current?.close()}
        className="m-auto rounded-md border border-border bg-surface p-0 text-fg backdrop:bg-black/40"
      >
        <div className="flex w-96 flex-col gap-3 p-4">
          <h2 className="text-lg font-semibold">Pounce</h2>
          <p className="text-md text-fg-muted">
            A technical SEO crawler. Native, so a crawl of a hundred thousand
            pages is a coffee break rather than an afternoon.
          </p>
          <dl className="tabular flex flex-col gap-1 text-sm">
            <Row
              label="Version"
              value={info ? info.version : "no engine — run the desktop shell"}
            />
            {info && (
              <>
                <Row label="Checks" value={`${info.rules} rules`} />
                <Row label="File format" value={`schema ${info.schemaVersion}`} />
              </>
            )}
            {handle && (
              <Row
                label="This crawl"
                value={`schema ${handle.schemaVersion}${
                  info && handle.schemaVersion !== info.schemaVersion
                    ? " — written by a different build"
                    : ""
                }`}
              />
            )}
          </dl>
          <button
            onClick={() => dialog.current?.close()}
            className="btn self-end"
          >
            Close
          </button>
        </div>
      </dialog>
    </>
  );
}

function Row({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex justify-between gap-4">
      <dt className="text-fg-faint">{label}</dt>
      <dd className="text-fg">{value}</dd>
    </div>
  );
}
