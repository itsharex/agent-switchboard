import { actionProps } from "../content/site-content";
import { useSitePreferences } from "../use-site-preferences";
import { AppScreenshot } from "./AppScreenshot";
import { GithubIcon } from "./icons";

export function Hero() {
  const { content } = useSitePreferences();
  const actions = content.hero.actions;
  return (
    <section className="hero">
      <div className="hero-ambient" aria-hidden="true">
        <img
          src="/media/ambient/abstract-desktop-product-background.webp"
          alt=""
          decoding="async"
          fetchPriority="high"
        />
        <div className="hero-ambient-veil" />
      </div>
      <div className="site-container hero-inner">
        <div className="hero-copy">
          <h1>{content.hero.title}</h1>
          <p className="hero-fact">{content.hero.fact}</p>
          <div className="hero-actions">
            {actions.map((action) => (
              <a
                key={action.label}
                className={`site-btn site-btn-${action.variant}`}
                {...actionProps(action)}
              >
                {action.icon === "github" ? <GithubIcon size={15} /> : null}
                {action.label}
              </a>
            ))}
          </div>
        </div>
        <div className="hero-shot">
          <AppScreenshot shot={content.hero.shot} eager />
        </div>
      </div>
    </section>
  );
}
