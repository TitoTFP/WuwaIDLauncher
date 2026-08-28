<script lang="ts">
  import { marked } from 'marked';
  import { sanitizeReleaseNotesHtml } from '../lib/sanitize';
  import type { ReleaseNotePayload } from '../lib/types';

  interface Props {
    note: ReleaseNotePayload | null;
    onclose: () => void;
  }

  let { note, onclose }: Props = $props();

  let parsedHtml = $derived.by(() => {
    const fallback = '<p>Catatan rilis belum tersedia. Launcher berhasil diperbarui.</p>';
    if (!note?.body) return fallback;
    try {
      const html = sanitizeReleaseNotesHtml(marked.parse(note.body, { async: false }) as string);
      return html.trim() ? html : fallback;
    } catch {
      const html = sanitizeReleaseNotesHtml(note.body);
      return html.trim() ? html : fallback;
    }
  });
</script>

{#if note}
  <div class="patch-notes-overlay" role="presentation">
    <dialog open class="patch-notes-modal" aria-modal="true" aria-labelledby="patch-notes-title">
      <div class="patch-notes-modal__head">
        <div>
          <p class="patch-notes-modal__eyebrow">WHAT'S NEW</p>
          <p class="patch-notes-modal__status">PEMBARUAN BERHASIL DITERAPKAN</p>
          <h2 id="patch-notes-title">{note.title || 'Pembaruan launcher'}</h2>
          <p class="patch-notes-modal__meta">
            {note.tag}{note.date ? ` · ${note.date}` : ''}{note.author ? ` · ${note.author}` : ''}
          </p>
        </div>
        <button class="patch-notes-modal__close" type="button" aria-label="Tutup patch notes" onclick={onclose}>×</button>
      </div>
      <div class="patch-notes-modal__body">
        <!-- eslint-disable-next-line svelte/no-at-html-tags -->
        {@html parsedHtml}
      </div>
      <div class="patch-notes-modal__actions">
        <button type="button" onclick={onclose}>Mengerti</button>
      </div>
    </dialog>
  </div>
{/if}

<style>
  .patch-notes-overlay {
    position: fixed;
    inset: 0;
    z-index: 900;
    display: grid;
    place-items: center;
    padding: 28px;
    background: rgba(3, 18, 21, 0.78);
    backdrop-filter: blur(7px) saturate(1.08);
    -webkit-backdrop-filter: blur(7px) saturate(1.08);
  }

  .patch-notes-modal {
    position: relative;
    width: min(720px, 100%);
    max-height: min(680px, 90vh);
    display: flex;
    flex-direction: column;
    overflow: hidden;
    margin: 0;
    color: var(--text-1);
    background: rgba(7, 26, 30, 0.96);
    border: 1px solid var(--mist-line-strong);
    border-radius: 0;
    clip-path: polygon(
      0 0,
      calc(100% - 18px) 0,
      100% 18px,
      100% calc(100% - 18px),
      calc(100% - 18px) 100%,
      0 100%
    );
    box-shadow:
      0 24px 70px rgba(0, 0, 0, 0.72),
      0 0 34px rgba(121, 203, 208, 0.14),
      inset 0 1px 0 rgba(236, 255, 249, 0.08);
  }

  .patch-notes-modal::after {
    position: absolute;
    right: 18px;
    bottom: 8px;
    width: 34px;
    height: 1px;
    content: '';
    background: var(--mist-lantern);
    box-shadow: 0 0 8px rgba(231, 211, 148, 0.42);
    pointer-events: none;
  }

  .patch-notes-modal__head {
    display: flex;
    justify-content: space-between;
    gap: 20px;
    padding: 22px 24px 16px;
    border-bottom: 1px solid var(--mist-line);
  }

  .patch-notes-modal__eyebrow {
    margin: 0 0 6px;
    color: var(--mist-lantern);
    letter-spacing: 0.18em;
    font-size: 10px;
    font-weight: 800;
  }

  .patch-notes-modal__status {
    margin: 0 0 6px;
    color: var(--mist-jade);
    font-size: 9px;
    font-weight: 800;
    letter-spacing: 0.12em;
  }

  h2 {
    margin: 0 0 6px;
    color: var(--mist-aqua);
    font-family: "Cormorant Garamond", "Noto Serif", Georgia, serif;
    font-size: 22px;
    font-weight: 700;
    letter-spacing: 0.04em;
  }

  .patch-notes-modal__meta {
    margin: 0;
    color: var(--mist-slate);
    font-size: 11px;
  }

  .patch-notes-modal__close {
    width: 32px;
    height: 32px;
    flex: 0 0 auto;
    border: 1px solid var(--mist-line);
    border-radius: 0;
    clip-path: polygon(6px 0, 100% 0, 100% calc(100% - 6px), calc(100% - 6px) 100%, 0 100%, 0 6px);
    background: rgba(170, 214, 217, 0.06);
    color: var(--mist-aqua);
    font-size: 22px;
    cursor: var(--cursor-select);
  }

  .patch-notes-modal__close:hover {
    background: rgba(170, 214, 217, 0.14);
    color: var(--mist-jade);
  }

  .patch-notes-modal__body {
    overflow: auto;
    padding: 22px 24px;
    color: var(--text-2);
    line-height: 1.55;
  }

  .patch-notes-modal__body :global(h1),
  .patch-notes-modal__body :global(h2),
  .patch-notes-modal__body :global(h3) {
    color: var(--mist-jade);
  }

  .patch-notes-modal__body :global(a) {
    color: var(--mist-aqua);
  }

  .patch-notes-modal__actions {
    display: flex;
    justify-content: flex-end;
    padding: 14px 24px 20px;
    border-top: 1px solid var(--mist-line);
  }

  .patch-notes-modal__actions button {
    border: 1px solid rgba(231, 211, 148, 0.78);
    border-radius: 0;
    clip-path: polygon(8px 0, 100% 0, calc(100% - 8px) 100%, 0 100%);
    padding: 9px 18px;
    background: var(--mist-grad);
    color: var(--mist-ink);
    font-weight: 800;
    cursor: var(--cursor-select);
  }

  .patch-notes-modal__actions button:hover {
    filter: brightness(1.06);
    box-shadow: 0 0 18px rgba(170, 214, 217, 0.3);
  }

  @media (max-width: 600px) {
    .patch-notes-overlay { padding: 12px; }
    .patch-notes-modal__head,
    .patch-notes-modal__body { padding-left: 16px; padding-right: 16px; }
  }
</style>
