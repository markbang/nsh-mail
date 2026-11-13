// A message's HTML in the platform's web view, with no scripts (the engine
// sanitized it once, and the view runs none): remote content stays blocked by
// the page's CSP until the reader asks, links open outside the app, and the
// mail keeps its own colors on a light card in dark mode too.
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

function mailDocument(html: string, remote: boolean) {
  const from = remote ? " https: http:" : "";
  const csp = `default-src 'none'; img-src data:${from}; style-src 'unsafe-inline'${from}; font-src data:${from}`;
  // The web's view is a sandboxed iframe: links open in a new tab, away from the app.
  const base = platform.os === "web" ? `<base target="_blank">` : "";
  return `<!doctype html><html><head><meta charset="utf-8">
<meta http-equiv="Content-Security-Policy" content="${csp}">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="color-scheme" content="light only">${base}
<style>html{background:#fff;color:#202020}body{margin:0;padding:16px;font:15px/1.5 system-ui,-apple-system,"Segoe UI",Roboto,"Noto Sans","PingFang SC","Microsoft YaHei",sans-serif;overflow-wrap:break-word}img{max-width:100%;height:auto}pre{white-space:pre-wrap}blockquote{margin:0 0 0 4px;padding-left:12px;border-left:2px solid #d4d4d8;color:#52525b}</style>
</head><body>${html}</body></html>`;
}
