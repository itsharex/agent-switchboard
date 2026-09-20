import type { SiteScreenshot } from "../content/site-content";

/** One real sandbox screenshot presented in the site's framed surface. */
export function AppScreenshot({ shot, eager = false }: {
  shot: SiteScreenshot;
  eager?: boolean;
}) {
  return (
    <figure className="app-shot">
      <img
        src={shot.src}
        alt={shot.alt}
        width={shot.width}
        height={shot.height}
        decoding={eager ? "sync" : "async"}
        loading={eager ? "eager" : "lazy"}
        {...(eager ? { fetchPriority: "high" as const } : {})}
      />
      <figcaption className="app-shot-caption">
        <span className="app-shot-badge">{shot.badge}</span>
        <span className="app-shot-note">{shot.caption}</span>
      </figcaption>
    </figure>
  );
}
