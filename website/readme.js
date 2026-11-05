import { execFileSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { marked } from "marked";

// The site root is website/, so README images have to be copied into public/
// to be served in dev and copied by the build.
const SITE_ROOT = dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = join(SITE_ROOT, "..");
const PUBLIC_DIR = join(SITE_ROOT, "public", "assets");
const REPO_URL = "https://github.com/dcog989/Cliptoo/blob/main";
const REPO_ASSET = /!\[([^\]]*)\]\((?:assets|static)\/([^)]+)\)/g;
// The brand mark is already in the header, so the README's inline copy is dropped.
const BRAND_MARK = /!\[[^\]]*\]\(assets\/cliptoo-48\.svg\)\s*/gi;
// Carousel slides render at about 430 px wide, so 960 px covers 2x displays.
const SLIDE_WIDTHS = [480, 960];

/** ImageMagick is a build tool, not a package dependency; without it the
 *  original file is served and only the responsive variants are skipped. */
function imageCli() {
  for (const cli of ["magick", "convert"]) {
    try {
      execFileSync(cli, ["-version"], { stdio: "ignore" });
      return cli;
    } catch {}
  }
  return null;
}

/** Writes resized copies so phones do not download a 1928 px screenshot. */
function resizeVariants(cli, source, file) {
  const stem = file.replace(/\.[^.]+$/, "");
  return SLIDE_WIDTHS.map((width) => {
    const name = `${stem}-${width}w.webp`;
    execFileSync(cli, [source, "-resize", `${width}x`, "-quality", "80", join(PUBLIC_DIR, name)]);
    return { name, width };
  });
}

/** README images live outside the site root; copy them and point at the copy. */
function localAssets(markdown) {
  const files = new Set([...markdown.matchAll(REPO_ASSET)].map((match) => match[2]));
  if (files.size > 0) mkdirSync(PUBLIC_DIR, { recursive: true });

  for (const file of files) {
    const source = [join(REPO_ROOT, "assets", file), join(REPO_ROOT, "static", file)].find(existsSync);
    if (source) copyFileSync(source, join(PUBLIC_DIR, file));
  }

  return markdown.replace(REPO_ASSET, (_match, alt, file) => `![${alt}](assets/${file})`);
}

/** marked emits headings without ids; the README's in-page links need them. */
function headingIds(html) {
  const seen = new Map();
  return html.replace(/<h([1-6])>([\s\S]*?)<\/h\1>/g, (_match, level, inner) => {
    const base =
      inner
        .replace(/<[^>]+>/g, "")
        .trim()
        .toLowerCase()
        .replace(/[^\p{L}\p{N}\s-]/gu, "")
        .replace(/\s+/g, "-") || "section";
    const count = seen.get(base) ?? 0;
    seen.set(base, count + 1);
    return `<h${level} id="${count === 0 ? base : `${base}-${count}`}">${inner}</h${level}>`;
  });
}

/** The opening heading and tagline take the scaffold's hero typography. */
function hero(html) {
  return html
    .replace(/<h1([^>]*)>/, '<h1$1 class="hero__title text-balance">')
    .replace(/<p>/, '<p class="hero__lead">')
    .replace(
      /(<h1[^>]*>[\s\S]*?<\/h1>)\s*(<p class="hero__lead">[\s\S]*?<\/p>)/,
      '<section class="hero flow">$1$2</section>',
    );
}

/** README links are repo-relative; on the site they point at the repo. */
function repoLinks(html) {
  return html.replace(/href="(?!https?:|#|mailto:|\/)([^"]+)"/g, `href="${REPO_URL}/$1"`);
}

/** The README's run of screenshots becomes the scaffold's carousel. */
/** src + srcset for a slide, falling back to the original file. */
function sources(shot) {
  if (!shot.variants?.length) return ` src="assets/${shot.file}"`;
  const largest = shot.variants[shot.variants.length - 1];
  const set = shot.variants.map((variant) => `assets/${variant.name} ${variant.width}w`).join(", ");
  return ` src="assets/${largest.name}" srcset="${set}" sizes="(max-width: 640px) 92vw, 78vw"`;
}

function carousel(screenshots) {
  if (screenshots.length === 0) return "";

  const slides = screenshots
    .map(
      (
        shot,
        i,
      ) => `<div class="carousel__slide${i === 0 ? " is-active" : ""}" role="group" aria-roledescription="slide" aria-label="${
        i + 1
      } of ${screenshots.length}"${i === 0 ? "" : ' aria-hidden="true"'}>
                <button class="carousel__zoom" type="button" aria-label="View screenshot ${i + 1} full size">
                <img${sources(shot)} alt="${shot.alt}"${shot.width ? ` width="${shot.width}" height="${shot.height}"` : ""}${
                  i === 0 ? ' fetchpriority="high" decoding="async"' : ' loading="lazy" decoding="async"'
                }>
                </button>
              </div>`,
    )
    .join("\n              ");

  return `<div class="carousel" data-carousel role="group" aria-roledescription="carousel" aria-label="Cliptoo screenshots">
            <div class="carousel__viewport">
              <div class="carousel__track">
              ${slides}
              </div>
              <div class="carousel__controls">
                <button class="carousel__control carousel__control--prev" type="button" data-carousel-prev aria-label="Previous slide">
                  <svg class="carousel__icon" viewBox="0 0 24 24" width="20" height="20" aria-hidden="true" focusable="false">
                    <path fill="currentColor" d="M15.4 7.4 14 6l-6 6 6 6 1.4-1.4L10.8 12z"></path>
                  </svg>
                </button>
                <button class="carousel__control carousel__control--next" type="button" data-carousel-next aria-label="Next slide">
                  <svg class="carousel__icon" viewBox="0 0 24 24" width="20" height="20" aria-hidden="true" focusable="false">
                    <path fill="currentColor" d="m8.6 16.6 1.4 1.4 6-6-6-6-1.4 1.4L13.2 12z"></path>
                  </svg>
                </button>
                <button class="carousel__control carousel__toggle" type="button" data-carousel-toggle aria-label="Pause slideshow">
                  <svg class="carousel__icon carousel__icon--pause" viewBox="0 0 24 24" width="20" height="20" aria-hidden="true" focusable="false">
                    <path fill="currentColor" d="M7 5h4v14H7zM13 5h4v14h-4z"></path>
                  </svg>
                  <svg class="carousel__icon carousel__icon--play" viewBox="0 0 24 24" width="20" height="20" aria-hidden="true" focusable="false">
                    <path fill="currentColor" d="M8 5v14l11-7z"></path>
                  </svg>
                </button>
              </div>
            </div>
          </div>`;
}

export function renderReadme() {
  const source = readFileSync(join(REPO_ROOT, "README.md"), "utf8");

  // The screenshot run is the only line made of nothing but images.
  const run = source.split("\n").find((line) => /^(?:\s*!\[[^\]]*\]\([^)]+\)\s*)+$/.test(line) && line.includes("!["));
  const screenshots = run
    ? [...run.matchAll(/!\[([^\]]*)\]\((?:assets|static)\/([^)]+)\)/g)].map((match) => ({
        alt: match[1],
        file: match[2].split("/").pop(),
      }))
    : [];

  // Strip before copying: the brand mark is in the header and the screenshots
  // become the carousel, so neither original needs to ship.
  const markdown = localAssets(
    source
      .replace(BRAND_MARK, "")
      // GitHub admonition markers are not Markdown; keep the note as a quote.
      .replace(/^> \[!NOTE\][ \t]*/gim, "> **Note:** ")
      .replace(/^(?:\s*!\[[^\]]*\]\([^)]+\)\s*)+$/m, "<!--SCREENSHOTS-->"),
  );

  const cli = screenshots.length > 0 ? imageCli() : null;
  if (screenshots.length > 0) mkdirSync(PUBLIC_DIR, { recursive: true });
  for (const shot of screenshots) {
    const source = join(REPO_ROOT, "assets", shot.file);
    if (!existsSync(source)) continue;
    // Without ImageMagick the original is shipped as-is; a missing copy would
    // leave the carousel pointing at files that were never written.
    if (!cli) {
      copyFileSync(source, join(PUBLIC_DIR, shot.file));
      continue;
    }
    shot.variants = resizeVariants(cli, source, shot.file);
    const largest = shot.variants[shot.variants.length - 1];
    const identify = ["-format", "%w %h", join(PUBLIC_DIR, largest.name)];
    const output =
      cli === "magick" ? execFileSync("magick", ["identify", ...identify]) : execFileSync("identify", identify);
    const [width, height] = output.toString().trim().split(" ").map(Number);
    shot.width = width;
    shot.height = height;
  }

  const html = hero(headingIds(repoLinks(marked.parse(markdown, { gfm: true, async: false }))));
  return html.replace("<!--SCREENSHOTS-->", carousel(screenshots));
}

/** Injects the rendered README into index.html in dev and in the build. */
export function readmePlugin() {
  let rendered = null;
  return {
    name: "cliptoo-readme",
    // Write the images before Vite copies public/, otherwise the copies that
    // happen during transformIndexHtml are too late for the build output.
    buildStart() {
      rendered = renderReadme();
    },
    transformIndexHtml(html, ctx) {
      if (!ctx.filename.endsWith("/index.html")) return html;
      rendered ??= renderReadme();
      return html.replace("<!--README-->", rendered);
    },
  };
}
