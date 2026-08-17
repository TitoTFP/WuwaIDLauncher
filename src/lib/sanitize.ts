const allowedTags = new Set([
  'p', 'br', 'strong', 'em', 'b', 'i', 'u', 's', 'ul', 'ol', 'li',
  'h1', 'h2', 'h3', 'blockquote', 'code', 'pre', 'a',
]);

function safeHref(value: string): string | null {
  const href = value.trim();
  if (/^https?:/i.test(href)) return href;
  if (/^\/(?!\/)/.test(href) || href.startsWith('#')) return href;
  return null;
}

export function sanitizeReleaseNotesHtml(input: string): string {
  let html = input.replace(/<!--[\s\S]*?-->/g, '');
  html = html.replace(
    /<\s*(script|style|iframe|object|embed|form|meta|link|base)\b[^>]*>[\s\S]*?<\s*\/\s*\1\s*>/gi,
    '',
  );

  return html.replace(/<\s*(\/?)\s*([a-z0-9-]+)([^>]*)>/gi, (_match, closing, rawTag, rawAttrs) => {
    const tag = String(rawTag).toLowerCase();
    if (!allowedTags.has(tag)) return '';
    if (closing) return '</' + tag + '>';
    if (tag === 'br') return '<br>';
    if (tag !== 'a') return '<' + tag + '>';

    const hrefMatch = String(rawAttrs).match(/\bhref\s*=\s*(['"])(.*?)\1/i);
    const href = hrefMatch ? safeHref(hrefMatch[2]) : null;
    const titleMatch = String(rawAttrs).match(/\btitle\s*=\s*(['"])(.*?)\1/i);
    const title = titleMatch?.[2]?.replace(/["<>]/g, '');
    const attrs = href
      ? ' href="' + href.replace(/["<>]/g, '') + '" target="_blank" rel="noopener noreferrer"' + (title ? ' title="' + title + '"' : '')
      : '';
    return '<a' + attrs + '>';
  });
}
