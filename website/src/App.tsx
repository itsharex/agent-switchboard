import { useEffect } from "react";

import { AppScreenshot } from "./components/AppScreenshot";
import { FinalCta } from "./components/FinalCta";
import { Hero } from "./components/Hero";
import { SiteHeader } from "./components/SiteHeader";
import { WriteLifecycle } from "./components/WriteLifecycle";
import type { SiteCapability, SiteShowcase } from "./content/site-content";
import { SitePreferencesProvider } from "./site-preferences";
import { useSitePreferences } from "./use-site-preferences";

function useReveal() {
  useEffect(() => {
    const elements = Array.from(document.querySelectorAll<HTMLElement>(".reveal"));
    if (!("IntersectionObserver" in window)) {
      for (const element of elements) element.classList.add("is-visible");
      return;
    }
    const observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (entry.isIntersecting) {
            entry.target.classList.add("is-visible");
            observer.unobserve(entry.target);
          }
        }
      },
      { threshold: 0.15 },
    );
    for (const element of elements) observer.observe(element);
    return () => observer.disconnect();
  }, []);
}

function Showcase({ id, reverse = false, showcase }: {
  id: string;
  reverse?: boolean;
  showcase: SiteShowcase;
}) {
  return (
    <section id={id} className="site-section">
      <div className={`site-container site-split${reverse ? " site-split-reverse" : ""}`}>
        <div className="site-split-copy reveal">
          <h2>{showcase.title}</h2>
          <p>{showcase.description}</p>
          <ul className="site-bullets">
            {showcase.bullets.map((bullet) => (
              <li key={bullet}>{bullet}</li>
            ))}
          </ul>
        </div>
        <div className="reveal">
          <AppScreenshot shot={showcase.shot} />
        </div>
      </div>
    </section>
  );
}

function CapabilityPanel({ capability }: { capability: SiteCapability }) {
  return (
    <article className="capability-panel reveal">
      <div className="capability-panel-head">
        <h3>{capability.title}</h3>
      </div>
      <p>{capability.description}</p>
      <ul className="site-bullets">
        {capability.bullets.map((bullet) => (
          <li key={bullet}>{bullet}</li>
        ))}
      </ul>
      {capability.shot ? (
        <div className="capability-panel-shot">
          <AppScreenshot shot={capability.shot} />
        </div>
      ) : null}
    </article>
  );
}

function SitePage() {
  const { content } = useSitePreferences();
  useReveal();
  return (
    <>
      <SiteHeader />
      <main>
        <Hero />
        <Showcase id="preview" showcase={content.preview} />
        <Showcase id="configuration" reverse showcase={content.configuration} />
        <section id="capabilities" className="site-section">
          <div className="site-container">
            <div className="section-intro reveal">
              <h2>{content.capabilities.title}</h2>
              <p>{content.capabilities.description}</p>
            </div>
            <div className="capability-row">
              <CapabilityPanel capability={content.capabilities.gateway} />
              <CapabilityPanel capability={content.capabilities.subagent} />
            </div>
          </div>
        </section>
        <section id="safety" className="site-section">
          <div className="site-container">
            <div className="reveal">
              <WriteLifecycle />
            </div>
          </div>
        </section>
        <FinalCta />
      </main>
    </>
  );
}

export default function App() {
  return (
    <SitePreferencesProvider>
      <SitePage />
    </SitePreferencesProvider>
  );
}
