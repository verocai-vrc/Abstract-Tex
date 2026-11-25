<script lang="ts">
  // Help (S17.16): the first questions, answered on one page that needs no internet.
  import { words } from '../lib/words';
</script>

<!-- A Tab stop, unlike the other pages' titles: the keys that scroll act on what has the focus, and there is
     nothing else in this page to give it to. -->
<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<h1 tabindex="0">{words.helpTitle}</h1>
{#each words.help as { question, answer } (question)}
  <section>
    <h2>{question}</h2>
    <p>{answer}</p>
  </section>
{/each}

<style>
  section {
    max-width: 640px;
    margin-top: 28px;
  }
  p {
    margin-top: 6px;
    overflow-wrap: anywhere;
    user-select: text;
  }
</style>
