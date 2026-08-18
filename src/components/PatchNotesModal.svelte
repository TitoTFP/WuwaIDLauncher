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
    if (!note?.body) return '<p>Belum ada isi patch notes.</p>';
    try {
      return sanitizeReleaseNotesHtml(marked.parse(note.body, { async: false }) as string);
    } catch {
      return sanitizeReleaseNotesHtml(note.body);
    }
  });
</script>

{#if note}
  <div class="patch-notes-overlay" role="presentation">
    <dialog open class="patch-notes-modal" aria-labelledby="patch-notes-title">
      <div class="patch-notes-modal__head">
        <div>
          <p class="patch-notes-modal__eyebrow">PATCH NOTES</p>
          <h2 id="patch-notes-title">{note.title || 'Pembaruan launcher'}</h2>
          <p class="patch-notes-modal__meta">{note.tag}{note.author ? ` · ${note.author}` : ''}</p>
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
  .patch-notes-overlay { position: fixed; inset: 0; z-index: 900; display: grid; place-items: center; padding: 28px; background: rgba(4, 7, 15, .72); }
  .patch-notes-modal { width: min(720px, 100%); max-height: min(680px, 90vh); display: flex; flex-direction: column; overflow: hidden; color: #f6f0df; background: rgba(14, 18, 32, .98); border: 1px solid rgba(220, 188, 112, .55); border-radius: 14px; box-shadow: 0 18px 60px rgba(0,0,0,.55); }
  .patch-notes-modal__head { display: flex; justify-content: space-between; gap: 20px; padding: 22px 24px 16px; border-bottom: 1px solid rgba(220, 188, 112, .18); }
  .patch-notes-modal__eyebrow { margin: 0 0 6px; color: #dcb86b; letter-spacing: .18em; font-size: 11px; }
  h2 { margin: 0 0 6px; font-size: 22px; }
  .patch-notes-modal__meta { margin: 0; color: #aeb2bd; font-size: 12px; }
  .patch-notes-modal__close { width: 32px; height: 32px; flex: 0 0 auto; border: 1px solid rgba(220,188,112,.35); border-radius: 50%; background: transparent; color: #f6f0df; font-size: 22px; cursor: pointer; }
  .patch-notes-modal__body { overflow: auto; padding: 22px 24px; color: #d6d8df; line-height: 1.55; }
  .patch-notes-modal__body :global(h1), .patch-notes-modal__body :global(h2), .patch-notes-modal__body :global(h3) { color: #e2c780; }
  .patch-notes-modal__body :global(a) { color: #e2c780; }
  .patch-notes-modal__actions { display: flex; justify-content: flex-end; padding: 14px 24px 20px; border-top: 1px solid rgba(220, 188, 112, .18); }
  .patch-notes-modal__actions button { border: 1px solid rgba(220, 188, 112, .5); border-radius: 7px; padding: 9px 18px; background: rgba(220,188,112,.14); color: #f6f0df; cursor: pointer; }
  .patch-notes-modal__actions button:hover, .patch-notes-modal__close:hover { background: rgba(220,188,112,.25); }
  @media (max-width: 600px) { .patch-notes-overlay { padding: 12px; } .patch-notes-modal__head, .patch-notes-modal__body { padding-left: 16px; padding-right: 16px; } }
</style>
