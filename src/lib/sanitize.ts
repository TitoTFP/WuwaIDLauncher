const allowedTags = new Set([
  'p', 'br', 'strong', 'em', 'b', 'i', 'u', 's', 'ul', 'ol', 'li',
  'h1', 'h2', 'h3', 'blockquote', 'code', 'pre', 'a',
]);

const blockedTags = new Set([
  'script', 'style', 'iframe', 'object', 'embed', 'form', 'meta', 'link', 'base',
]);

function safeHref(value: string): string | null {
  const href = value.trim();
  if (href.startsWith('#') || /^\/(?!\/)/.test(href)) return href;

  try {
    const parsed = new URL(href);
    return parsed.protocol === 'https:' || parsed.protocol === 'http:' ? parsed.href : null;
  } catch {
    return null;
  }
}

function appendSanitizedChildren(source: Node, target: Node, document: Document) {
  for (const child of Array.from(source.childNodes)) {
    const sanitized = sanitizeNode(child, document);
    if (sanitized) target.appendChild(sanitized);
  }
}

function sanitizeNode(node: Node, document: Document): Node | null {
  if (node.nodeType === Node.TEXT_NODE) {
    return document.createTextNode(node.textContent ?? '');
  }
  if (node.nodeType !== Node.ELEMENT_NODE) return null;

  const element = node as Element;
  const tag = element.tagName.toLowerCase();
  if (blockedTags.has(tag)) return null;

  if (!allowedTags.has(tag)) {
    const fragment = document.createDocumentFragment();
    appendSanitizedChildren(element, fragment, document);
    return fragment;
  }

  const output = document.createElement(tag);
  if (tag === 'a') {
    const href = safeHref(element.getAttribute('href') ?? '');
    if (href) output.setAttribute('href', href);
    const title = element.getAttribute('title');
    if (title) output.setAttribute('title', title);
    if (href && /^https?:/i.test(href)) {
      output.setAttribute('target', '_blank');
      output.setAttribute('rel', 'noopener noreferrer');
    }
  }

  appendSanitizedChildren(element, output, document);
  return output;
}

export function sanitizeReleaseNotesHtml(input: string): string {
  if (typeof DOMParser === 'undefined') {
    return input.replace(/[&<>"']/g, (character) => ({
      '&': '&amp;',
      '<': '&lt;',
      '>': '&gt;',
      '"': '&quot;',
      "'": '&#39;',
    })[character] ?? character);
  }

  const parsed = new DOMParser().parseFromString(input, 'text/html');
  const fragment = parsed.createDocumentFragment();
  appendSanitizedChildren(parsed.body, fragment, parsed);
  const container = parsed.createElement('div');
  container.appendChild(fragment);
  return container.innerHTML;
}
