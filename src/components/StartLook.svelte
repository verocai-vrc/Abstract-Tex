<script lang="ts">
  // Text and colours (S17.11): how the window looks on this computer. Every choice is made at once.
  import { COLOURS, look, setColours, setTextSize, TEXT_SIZES } from '../lib/look.svelte';
  import { words } from '../lib/words';
</script>

<h1 tabindex="-1">{words.lookTitle}</h1>
<section>
  <h2 id="start-look-text">{words.textSize}</h2>
  <p class="muted" id="start-look-size">{words.textSizeHint}</p>
  <div class="choices" role="radiogroup" aria-labelledby="start-look-text" aria-describedby="start-look-size">
    {#each TEXT_SIZES as size (size)}
      <label>
        <input type="radio" name="text-size" value={size} checked={look.textSize === size} onchange={() => void setTextSize(size)} />
        {words.percent(size)}
      </label>
    {/each}
  </div>
</section>
<section>
  <h2 id="start-look-colours">{words.coloursTitle}</h2>
  <div class="choices" role="radiogroup" aria-labelledby="start-look-colours">
    {#each COLOURS as colours (colours)}
      <label>
        <input type="radio" name="colours" value={colours} checked={look.colours === colours} onchange={() => void setColours(colours)} />
        {words.colours[colours]}
      </label>
    {/each}
  </div>
</section>

<style>
  section {
    margin-top: 32px;
  }
  .choices {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
    margin-top: 12px;
  }
  label {
    display: flex;
    align-items: center;
    gap: 12px;
    min-height: 44px;
    padding: 0 18px;
    background: var(--bg-editor);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    cursor: pointer;
  }
  label:hover {
    background: var(--bg-selected);
  }
  label:has(:checked) {
    border-color: var(--accent);
    box-shadow: inset 0 0 0 1px var(--accent);
    font-weight: 600;
  }
  label:focus-within {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  /* The target is the label (so this need not be 44px). Drawn here and not by the system, whose own can be white on white. */
  label input {
    appearance: none;
    flex: none;
    width: 22px;
    height: 22px;
    min-height: 0;
    padding: 0;
    border: 2px solid var(--fg-muted);
    border-radius: 50%;
    background: var(--bg-editor);
  }
  label input:checked {
    border: 7px solid var(--accent);
  }
  label input:focus-visible {
    outline: none;
  }
</style>
