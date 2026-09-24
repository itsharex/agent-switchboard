import type { SkillRepository } from "../../api/extensions/skill-sources";
import { tr } from "../../i18n/current";

export function skillRepositoryName(value: string): string | null {
  let path = value.trim();
  if (/^https?:\/\//i.test(path)) {
    try {
      const url = new URL(path);
      if (url.protocol !== "https:" || url.hostname !== "github.com" || url.port ||
        url.username || url.password || url.search || url.hash) return null;
      path = url.pathname.slice(1);
    } catch { return null; }
  }
  path = path.replace(/\/$/, "").replace(/\.git$/, "");
  return /^[a-z\d](?:[a-z\d-]*[a-z\d])?\/[a-z\d_.-]+$/i.test(path) &&
    ![".", ".."].includes(path.split("/")[1]) ? path : null;
}

export function skillRepositoryLabel(source: { repo: string; refName?: string | null; subpath: string }) {
  const repo = skillRepositoryName(source.repo) ?? tr("extensions.repo.fallbackName");
  return [repo, source.refName ? `@${source.refName}` : "", source.subpath].filter(Boolean).join(" · ");
}

export function skillSourceUrl(repo: string, readmeUrl: string | null = null): string | null {
  if (readmeUrl) {
    try {
      const url = new URL(readmeUrl);
      if (url.protocol === "https:" && ["github.com", "skills.sh"].includes(url.hostname) &&
        !url.port && !url.username && !url.password && !url.search && !url.hash) return url.href;
    } catch { /* Invalid source links fall back to the canonical repository. */ }
  }
  const name = skillRepositoryName(repo);
  return name ? `https://github.com/${name}` : null;
}

export function sameSkillRepository(left: SkillRepository, right: SkillRepository) {
  return left.id === right.id && left.repo === right.repo && left.refName === right.refName &&
    left.subpath === right.subpath && left.enabled === right.enabled;
}
