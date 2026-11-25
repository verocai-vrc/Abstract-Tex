<script lang="ts">
  // Text and colours (S17.11): how the window looks on this computer. Every choice is made at once.
  import { look, setTextSize, TEXT_SIZES } from '../lib/look.svelte';
  import { words } from '../lib/words';
</script>

<h1 tabindex="-1">{words.lookTitle}</h1>
<fieldset>
  <legend><h2>{words.textSize}</h2></legend>
  <p class="muted" id="start-look-size">{words.textSizeHint}</p>
  <div class="choices" role="radiogroup" aria-label={words.textSize} aria-describedby="start-look-size">
    {#each TEXT_SIZES as size (size)}
      <label>
        <input type="radio" name="text-size" value={size} checked={look.textSize === size} onchange={() => void setTextSize(size)} />
        {words.percent(size)}
      </label>
    {/each}
  </div>
</fieldset>

<style>
  fieldset {
    min-width: 0;
    margin: 24px 0 0;
    padding: 0;
    border: 0;
  }
  legend {
    padding: 0;
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
  /* The target is the label. */
  input {
    width: 20px;
    height: 20px;
    padding: 0;
    accent-color: var(--accent);
  }
</style>
