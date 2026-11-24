<script lang="ts">
  // Open a document (S17.4: a folder, or from a Git address; files come with S17.8).
  import { pickAndOpenFolder, showCloneWindow } from '../lib/controller.svelte';
  import { words } from '../lib/words';
</script>

<h1 tabindex="-1">{words.openTitle}</h1>
<ul>
  <li>
    <button data-first aria-describedby="start-open-folder" onclick={() => void pickAndOpenFolder()}>{words.chooseFolder}</button>
    <span id="start-open-folder" class="muted">{words.chooseFolderHint}</span>
  </li>
  <li>
    <button onclick={() => showCloneWindow()}>{words.getFromGit}</button>
  </li>
</ul>

<style>
  ul {
    display: flex;
    flex-direction: column;
    gap: 16px;
    margin-top: 24px;
  }
  li {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 16px;
  }
</style>
