// A message's HTML in the platform's web view, with no scripts (the engine
// sanitized it once, and the view runs none): remote content stays blocked by
// the page's CSP until the reader asks, links open outside the app, and the
// mail keeps its own colors on a light card in dark mode too. Mail laid out
// for a fixed width (a newsletter's 600px table) is scaled down to fit the
// pane, as mail apps do.
import { openUrl, platform, signal, WebView } from "nsh";

export interface MailViewProps {
  html: string;
  /** Remote images and CSS load. */
  remote: boolean;
  /** No web engine here: show the text instead. */
  onUnavailable: () => void;
  class?: string;
}

// Without a web engine (a Linux runtime without WebKitGTK, Windows without WebView2).
export const webViewAvailable = signal(true);
WebView.available().then((ok) => webViewAvailable.set(ok), () => {});

const LINK = /^(https?|mailto|tel):/i;

export function MailView(props: MailViewProps) {
  return (
    <WebView
      html={mailDocument(props.html, props.remote)}
      javaScriptEnabled={false}
      incognito
      onError={(e) => {
        // A link: nsh.json grants web views no host, so the navigation is refused and reported here.
        const url = (e as { url?: string }).url;
        if (e.code === "ERR_ACCESS_DENIED" && url && LINK.test(url)) void openUrl(url).catch(() => {});
        else if (e.code === "ERR_FEATURE_UNAVAILABLE_ON_PLATFORM") props.onUnavailable();
      }}
      class={props.class}
      testID="body-html"
    />
  );
}

// Below this the text gets too small to read: the mail scrolls sideways instead.
const MIN_ZOOM = 0.6;
const ZOOM_STEP = 0.025;
// Our body padding, dropped once the mail fills the view; the gutter newsletters commonly leave around their fixed-width table.
const PAD = 16;
const GUTTER = 2 * 24;
const BLOCK = /<(?:table|td|th|div|center)\b[^>]*>/gi;
const WIDTH_ATTR = /\swidth\s*=\s*["']?\s*(\d+(?:\.\d+)?)(?:px)?(?=["'\s>])/i;
const CSS_WIDTH = /(?<![-\w])(?:min-)?width\s*:\s*(\d+(?:\.\d+)?)px/gi;
const STYLE = /<style\b[^>]*>([^]*?)<\/style>/gi;

/** The widest fixed width the mail's blocks declare, in CSS px (0: none). The view runs no
 * scripts to measure the page, so this is what the mail says of itself. */
function declaredWidth(html: string) {
  let widest = 0;
  const take = (css: string) => {
    for (const m of css.matchAll(CSS_WIDTH)) widest = Math.max(widest, Number(m[1]));
  };
  for (const [tag] of html.matchAll(BLOCK)) {
    widest = Math.max(widest, Number(WIDTH_ATTR.exec(tag)?.[1] ?? 0));
    take(tag);
  }
  for (const [, css] of html.matchAll(STYLE)) take(css);
  return widest;
}

/** Zooms the body so `width` fits the view, in steps chosen by the view's width, down to MIN_ZOOM.
 * Narrower than the mail and our padding, the padding goes first: a 600px newsletter fits a phone. */
function fitCss(width: number) {
  if (!width) return "";
  const full = width + GUTTER;
  let css = `@media (max-width:${full + 2 * PAD - 1}px){body{padding:0}}`;
  for (let i = 1; 1 - i * ZOOM_STEP >= MIN_ZOOM - 1e-9; i++) {
    const zoom = 1 - i * ZOOM_STEP;
    css += `@media (max-width:${Math.ceil(full * (zoom + ZOOM_STEP)) - 1}px){body{zoom:${zoom.toFixed(3)}}}`;
  }
  return css;
}

function mailDocument(html: string, remote: boolean) {
  const from = remote ? " https: http:" : "";
  const csp = `default-src 'none'; img-src data:${from}; style-src 'unsafe-inline'${from}; font-src data:${from}`;
  // The web's view is a sandboxed iframe: links open in a new tab, away from the app.
  const base = platform.os === "web" ? `<base target="_blank">` : "";
  return `<!doctype html><html><head><meta charset="utf-8">
<meta http-equiv="Content-Security-Policy" content="${csp}">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="color-scheme" content="light only">${base}
<style>html{background:#fff;color:#202020}body{margin:0;padding:${PAD}px;font:15px/1.5 system-ui,-apple-system,"Segoe UI",Roboto,"Noto Sans","PingFang SC","Microsoft YaHei","Noto Sans CJK SC","Source Han Sans SC",sans-serif;overflow-wrap:break-word}img{max-width:100%;height:auto}pre{white-space:pre-wrap}blockquote{margin:0 0 0 4px;padding-left:12px;border-left:2px solid #d4d4d8;color:#52525b}${fitCss(declaredWidth(html))}</style>
</head><body>${html}</body></html>`;
}
