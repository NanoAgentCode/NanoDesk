import { formatUserFacingError } from "../lib/notices";

/** Keep diagnostics available without expanding backend output into the normal UI. */
export default function FriendlyError({ error, fallback }: { error: string; fallback?: string }) {
  const summary = formatUserFacingError(error, fallback);
  return <div className="friendly-error">
    <p role="alert">{summary}</p>
    {error !== summary && <details>
      <summary>查看错误详情</summary>
      <pre style={{ whiteSpace: "pre-wrap", overflowWrap: "anywhere", maxHeight: 240, overflow: "auto", fontSize: 12 }}>{error}</pre>
    </details>}
  </div>;
}
