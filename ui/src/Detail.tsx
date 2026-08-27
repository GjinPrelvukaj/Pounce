import { useEffect, useState } from "react";
import { MiddleTruncate } from "./MiddleTruncate";
import { useDelayed } from "./useDelayed";
import { pageDetail, type PageDetail, type RuleInfo } from "./engine";
import { severity } from "./severity";

/// Absent is not empty, and the store has kept that distinction all the way
/// from the parser. This is where a person finally sees it: a missing meta
/// description is a defect, `content=""` is someone's decision, and a blank
/// cell would say neither.
function Value({ value }: { value: string | null | undefined }) {
  if (value === null || value === undefined)
    return <span className="text-fg-faint">None</span>;
  if (value === "") return <span className="text-warning">Empty</span>;
  return <span className="break-words">{value}</span>;
}

function Field({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <div className="flex flex-col gap-0.5">
      <span className="text-xs text-fg-faint">{label}</span>
      <span className="text-sm text-fg">{children}</span>
    </div>
  );
}

function Section({
  title,
  children,
}: {
  title: string;
  children: React.ReactNode;
}) {
  return (
    <section className="flex min-w-0 flex-col gap-2">
      <h3 className="text-xs font-semibold tracking-[0.07em] text-fg-faint uppercase">
        {title}
      </h3>
      {children}
    </section>
  );
}

function List({ items }: { items: string[] | null | undefined }) {
  if (!items || items.length === 0)
    return <span className="text-fg-faint">None</span>;
  return (
    <ol className="flex flex-col gap-0.5">
      {items.map((item, i) => (
        <li key={`${i}-${item}`} className="break-words">
          {item === "" ? <span className="text-warning">Empty</span> : item}
        </li>
      ))}
    </ol>
  );
}

/// One page, in full: what the crawler was told, what the markup said, what
/// links here and where it leads.
///
/// Fetched by id on demand rather than carried on the grid row — the row holds
/// nine columns because a million of them have to be sorted, and this holds
/// everything because there is exactly one.
export function Detail({
  id,
  rules,
  onClose,
}: {
  /// `null` when no row is selected. The pane still occupies its place and
  /// says "No URL selected", the way every other region of this app is
  /// visible before it has anything to show.
  id: number | null;
  rules: Map<string, RuleInfo>;
  onClose: () => void;
}) {
  const [page, setPage] = useState<PageDetail | null>(null);
  const [tab, setTab] = useState("details");

  // Counts on the tabs, because "Inlinks 0" and "Inlinks 4,312" are different
  // pages and you should not have to open one to find out which.
  const tabs = page
    ? [
        { id: "details", label: "Details", count: undefined },
        { id: "findings", label: "Findings", count: page.issues.length },
        { id: "inlinks", label: "Linked from", count: page.inlinkCount },
        { id: "outlinks", label: "Links to", count: page.outlinkCount },
      ]
    : [];
  const [error, setError] = useState<string | null>(null);
  const loading = useDelayed(id !== null && page === null && error === null);

  // Escape closes it. The pane is opened with Enter from the grid, and an
  // interaction you can enter with the keyboard and only leave with the mouse
  // is worse than one that was never keyboard-reachable.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  useEffect(() => {
    if (id === null) {
      setPage(null);
      setError(null);
      return;
    }
    let current = true;
    setError(null);
    pageDetail(id)
      .then((p) => {
        if (!current) return;
        setPage(p);
        if (p === null) setError("That page is no longer in this crawl.");
      })
      .catch((e) => current && setError(String((e as Error)?.message ?? e)));
    return () => {
      current = false;
    };
  }, [id]);

  return (
    // A definite height and `shrink-0`, not a max-height: the grid above is
    // `flex-1`, so it takes every pixel the pane does not insist on — and a
    // `min-h-0` pane in a contested column collapses to nothing, which is
    // exactly what it did the first time.
    // `h-full`, not a fixed viewport fraction. Since T4.41 this pane lives in a
    // resizable `Panel` that owns its height, and the old `h-[32vh] shrink-0`
    // fought it: the section stopped at 32% of the window while the panel kept
    // whatever the splitter gave it, so dragging the pane taller just revealed
    // more canvas underneath the content.
    <section className="pane-in raised-panel z-20 flex h-full min-h-0 flex-col border-t border-border bg-surface">
      <header className="flex items-center gap-2 border-b border-border px-4 py-2">
        {page && (
          <span
            className={`nums text-sm ${
              page.status >= 500
                ? "text-critical"
                : page.status >= 400
                  ? "text-warning"
                  : page.status >= 300
                    ? "text-notice"
                    : "text-pass"
            }`}
          >
            {page.status}
          </span>
        )}
        <span className="tabular min-w-0 flex-1 text-sm text-accent-fg">
          {page ? (
            <MiddleTruncate text={page.url} />
          ) : (
            // Left where the URL will be, not pushed to the far edge by an
            // empty slot: the label should sit where the thing it stands in
            // for is going to appear.
            <span className="font-sans font-medium text-fg-muted">
              No URL selected
            </span>
          )}
        </span>
        {id !== null && (
          <button
            onClick={onClose}
            aria-label="Close the detail pane"
            className="btn"
          >
            Close
          </button>
        )}
      </header>

      {error && <p className="px-4 py-3 text-sm text-critical">{error}</p>}

      {loading && !page && !error && (
        <div className="flex flex-col gap-3 p-4" aria-hidden>
          {Array.from({ length: 6 }, (_, i) => (
            <div
              key={i}
              className="h-3 rounded-sm bg-raised-2"
              style={{ width: `${60 - (i % 3) * 15}%` }}
            />
          ))}
        </div>
      )}

      {id === null && (
        <div className="flex flex-1 items-center justify-center">
          <p className="text-md text-fg-faint">
            Select a row to see everything about that page
          </p>
        </div>
      )}

      {page && (
        <div className="flex shrink-0 gap-1 border-b border-border px-3 pt-2">
          {tabs.map((t) => (
            <button
              key={t.id}
              onClick={() => setTab(t.id)}
              aria-pressed={tab === t.id}
              className="tab"
            >
              {t.label}
              {t.count !== undefined && (
                <span className="nums text-xs text-fg-faint">
                  {t.count.toLocaleString()}
                </span>
              )}
            </button>
          ))}
        </div>
      )}

      {page && tab === "details" && (
        <div className="grid min-h-0 flex-1 gap-6 overflow-auto p-4 [grid-template-columns:repeat(auto-fit,minmax(19rem,1fr))]">
          <Section title="What the page says">
            <Field label="Title">
              <Value value={page.title} />
            </Field>
            <Field label="Meta description">
              <Value value={page.metaDescription} />
            </Field>
            <Field label="H1">
              <List items={page.h1} />
            </Field>
            <Field label="H2">
              <List items={page.h2} />
            </Field>
            <Field label="Canonical">
              <span className="tabular">
                <Value value={page.canonical} />
              </span>
            </Field>
            <Field label="Robots directives">
              {[
                page.noindex && "noindex",
                page.nofollow && "nofollow",
                page.noarchive && "noarchive",
                page.nosnippet && "nosnippet",
              ].filter(Boolean).length === 0 ? (
                <span className="text-fg-muted">
                  None. The page allows indexing.
                </span>
              ) : (
                <span className="nums">
                  {[
                    page.noindex && "noindex",
                    page.nofollow && "nofollow",
                    page.noarchive && "noarchive",
                    page.nosnippet && "nosnippet",
                  ]
                    .filter(Boolean)
                    .join(", ")}
                </span>
              )}
            </Field>
            <Field label="Words">
              <span className="nums">{page.wordCount.toLocaleString()}</span>
            </Field>
            {page.hreflang && page.hreflang.length > 0 && (
              <Field label="Hreflang">
                <List
                  items={page.hreflang.map((h) => `${h.lang} → ${h.href}`)}
                />
              </Field>
            )}
            {page.openGraph && page.openGraph.length > 0 && (
              <Field label="Open Graph">
                <List items={page.openGraph.map(([k, v]) => `${k}: ${v}`)} />
              </Field>
            )}
          </Section>

          <Section title="How it was served">
            <Field label="Content type">
              <span className="nums">
                <Value value={page.contentType} />
                {page.contentTypeMismatch && (
                  <span className="ml-2 text-warning">
                    (the body does not look like this)
                  </span>
                )}
              </span>
            </Field>
            <Field label="Size">
              <span className="nums">
                {page.size.toLocaleString()} bytes
                {page.truncated && (
                  <span className="ml-2 text-warning">
                    (truncated, so the fields above may be incomplete)
                  </span>
                )}
              </span>
            </Field>
            <Field label="Time">
              <span className="nums">
                {page.elapsedMs.toLocaleString()} ms total,{" "}
                {page.timeToHeadersMs.toLocaleString()} ms to first byte
              </span>
            </Field>
            <Field label="Clicks from home">
              <span className="nums">{page.depth}</span>
            </Field>
            <Field label="Redirects taken to get here">
              {page.redirectChain && page.redirectChain.length > 0 ? (
                <span className="nums">
                  <List items={page.redirectChain} />
                </span>
              ) : (
                <span className="text-fg-muted">None (reached directly)</span>
              )}
            </Field>
            {page.images && page.images.length > 0 && (
              <Field
                label={`Images (${page.images.filter((i) => i.alt === null).length} with no alt)`}
              >
                <span className="nums">
                  <List
                    items={page.images
                      .slice(0, 20)
                      .map(
                        (img) =>
                          `${img.src}${img.alt === null ? "  (no alt)" : ""}`,
                      )}
                  />
                </span>
              </Field>
            )}
          </Section>

        </div>
      )}

      {page && tab === "findings" && (
        <div className="min-h-0 flex-1 overflow-auto p-4">
          <Section title={`Findings (${page.issues.length})`}>
            {page.issues.length === 0 ? (
              <p className="text-sm text-fg-muted">
                Nothing to fix on this page.
              </p>
            ) : (
              <ul className="flex flex-col gap-2">
                {page.issues.map((issue, i) => {
                  const sev = severity(issue.severity);
                  const rule = rules.get(issue.ruleId);
                  return (
                    <li key={`${issue.ruleId}-${i}`} className="flex gap-2">
                      <span aria-hidden className={`${sev.tone} shrink-0`}>
                        {sev.icon}
                      </span>
                      <div className="flex min-w-0 flex-col gap-0.5">
                        <span className="text-sm text-fg">
                          {rule?.description ?? issue.ruleId}
                        </span>
                        {issue.detail && (
                          <span className="nums text-xs break-words text-fg-muted">
                            {issue.detail}
                          </span>
                        )}
                        {rule && (
                          <span className="text-sm text-fg-muted">
                            {rule.remediation}
                          </span>
                        )}
                        <span className="nums text-xs text-fg-faint">
                          {sev.label} · {issue.ruleId}
                        </span>
                      </div>
                    </li>
                  );
                })}
              </ul>
            )}
          </Section>

        </div>
      )}

      {page && (tab === "inlinks" || tab === "outlinks") && (
        <div className="min-h-0 flex-1 overflow-auto p-4">
          {tab === "inlinks" ? (
            <Links rows={page.inlinks} total={page.inlinkCount} />
          ) : (
            <Links rows={page.outlinks} total={page.outlinkCount} />
          )}
        </div>
      )}
    </section>
  );
}

function Links({
  rows,
  total,
}: {
  rows: { url: string; anchorText: string; nofollow: boolean; crawled: boolean }[];
  total: number;
}) {
  if (rows.length === 0)
    return (
      <p className="text-sm text-fg-muted">
        None. No page in this crawl links here.
      </p>
    );
  return (
    <div className="flex flex-col gap-1">
      <ul className="flex flex-col gap-1">
        {rows.map((row, i) => (
          <li key={`${i}-${row.url}`} className="flex min-w-0 flex-col">
            <span
              className={`tabular text-sm ${row.crawled ? "text-accent-fg" : "text-fg-muted"}`}
              title={row.url}
            >
              <MiddleTruncate text={row.url} tailLength={20} />
            </span>
            <span className="text-xs text-fg-faint">
              {row.anchorText === "" ? (
                <span className="text-warning">no anchor text</span>
              ) : (
                `“${row.anchorText}”`
              )}
              {row.nofollow && " · nofollow"}
              {!row.crawled && " · not crawled"}
            </span>
          </li>
        ))}
      </ul>
      {/* The list is a sample, and saying so is the difference between a cap
          and a lie. */}
      {total > rows.length && (
        <span className="text-xs text-fg-faint">
          showing the first {rows.length} of {total.toLocaleString()}
        </span>
      )}
    </div>
  );
}
