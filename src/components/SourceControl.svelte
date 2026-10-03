<script lang="ts">
  // The Source Control view (S10.3a, DESIGN.md §6), the second tenant of the left pane. Copied
  // from VS Code top to bottom, and this loop builds the bottom half of it: the file lists and
  // their three verbs. The commit box, the Commit button and the Graph section are S10.3b.
  //
  // Everything here reads `git` and calls the controller; the panel itself decides nothing. Two
  // states are deliberately sentences rather than empty space — no project, and a folder that is
  // not a Git repository — because an empty panel would look like a clean tree, which is a
  // different and much more reassuring thing than "nobody asked Git anything".
  import {
    amendCommit,
    cancelGitHubSignIn,
    clearComparison,
    comparisonTitle,
    claimCommitMessage,
    commitAndPush,
    commitAndSync,
    commitStaged,
    createGitHubRepository,
    discardChange,
    dismissLargeFiles,
    ignoreOurFolder,
    initialiseRepository,
    loadMoreCommits,
    markGraphRow,
    openSnapshot,
    suggestedRemoteName,
    openFile,
    openVerificationPage,
    refreshGitStatus,
    signInToGitHub,
    signOutOfGitHub,
    stageChange,
    syncChanges,
    trackLargeFilesWithLfs,
    unstageChange,
  } from '../lib/controller.svelte';
  import { formatMegabytes, git, relativeTime, syncArrows, wordDeltaLabel, type ChangeRow } from '../lib/git.svelte';
  import { compare, markOf } from '../lib/compare.svelte';
  import { github, timeLeft } from '../lib/github.svelte';
  import { snapshotLabel, snapshots } from '../lib/snapshots.svelte';
  import { app } from '../lib/state.svelte';

  // S10.5b: the publish block's own two fields. `$state` and not `$derived`, because once the
  // author has typed a name it is theirs — the same rule the commit box follows.
  let remoteName = $state('');
  let wantPublic = $state(false);
  // Filled from the project folder's name when the block first has a project to name, and again
  // when the project changes; never over something already typed.
  let namedProject = $state<string | null>(null);
  $effect(() => {
    if (app.project && namedProject !== app.project.rootDir) {
      namedProject = app.project.rootDir;
      remoteName = suggestedRemoteName();
      wantPublic = false;
    }
  });

  /** Copying the code is a web API and not a Tauri plugin, so it lives here rather than in
   * `ipc.ts`. It can fail — an old webview, a denied permission — and the code is on screen in
   * full either way, so a failure only turns the button's own label back. */
  let copied = $state(false);
  async function copyCode() {
    const code = github.code?.userCode;
    if (!code) return;
    try {
      await navigator.clipboard.writeText(code);
      copied = true;
      setTimeout(() => (copied = false), 1500);
    } catch {
      copied = false;
    }
  }

  /** `Ctrl Enter` commits (DESIGN.md §6), bound on the box and not in `shortcuts.ts`: that table
   * is for chords that mean the same thing wherever focus is, and a global `Ctrl Enter` would
   * fire from inside CodeMirror while the author was writing LaTeX. */
  function onMessageKeydown(event: KeyboardEvent) {
    if (event.key === 'Enter' && (event.ctrlKey || event.metaKey)) {
      event.preventDefault();
      void commitStaged();
    }
  }

  /** Nothing staged means nothing to commit, and the crate would refuse — so the button says so
   * before it is pressed rather than after. Also false mid-sync: a push or a fast-forward touches
   * the same branch a commit would move, and the two should not race. */
  const canCommit = $derived(git.status.staged.length > 0 && git.message.trim().length > 0 && !git.committing && !git.syncing);

  /** *Amend* (S11.1c) needs none of what `canCommit` needs staged: a reword with nothing staged
   * is the whole reason the item exists. Only a message, `git.showAmend`'s own guardrail, and the
   * same "nothing else is already running" check every verb here makes. */
  const canAmendNow = $derived(git.showAmend && git.message.trim().length > 0 && !git.committing && !git.syncing);

  /** The dropdown opens if either of its two commit-shaped items could actually do something —
   * otherwise a caret that opens onto two disabled rows would be worse than one that does not
   * open at all. */
  const canOpenCommitMenu = $derived(canCommit || canAmendNow);

  /** The Commit button's dropdown (DESIGN.md §6: *Commit & Push*, *Commit & Sync*, *Amend*).
   * Closed by choosing an item, by Escape, or by clicking anywhere else. */
  let commitMenuOpen = $state(false);

  function runCommitMenuItem(action: () => Promise<void>) {
    commitMenuOpen = false;
    void action();
  }

  // Closes on a click anywhere else or on Escape, the way a native menu would. Only listens while
  // the menu is actually open, the same `$effect`-with-cleanup shape the clock below uses.
  $effect(() => {
    if (!commitMenuOpen) return;
    function onDocumentClick(event: MouseEvent) {
      if (!(event.target as HTMLElement).closest('.commit-split')) commitMenuOpen = false;
    }
    function onDocumentKeydown(event: KeyboardEvent) {
      if (event.key === 'Escape') commitMenuOpen = false;
    }
    document.addEventListener('click', onDocumentClick);
    document.addEventListener('keydown', onDocumentKeydown);
    return () => {
      document.removeEventListener('click', onDocumentClick);
      document.removeEventListener('keydown', onDocumentKeydown);
    };
  });

  // "4 minutes ago" on a row would otherwise still say that an hour later. One tick a minute,
  // only while this view is on screen, as the status bar's build clock does it — and once a
  // second while a sign-in code is counting down, which is the one thing here that changes
  // faster than the graph does.
  let now = $state(Date.now());
  $effect(() => {
    const every = github.stage === 'waiting' ? 1_000 : 60_000;
    const timer = setInterval(() => (now = Date.now()), every);
    return () => clearInterval(timer);
  });

  /** A deleted file has nothing to open, so its row is not a button. §6's "a click opens a diff"
   * arrives with the merge view (S11.7); until then a click opens the file itself. */
  /** `Esc` anywhere in the Graph clears both marks and leaves the comparison (A10). */
  function onGraphKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape' && (compare.marks.from !== null || compare.mode)) {
      event.preventDefault();
      clearComparison();
    }
  }

  /** The toolbar's account of what is marked: the pair as Rust ordered it once it knows, and
   * otherwise what was clicked — a lone *from* says what the next click will do. */
  const toolbarText = $derived.by(() => {
    if (compare.view.phase === 'building') {
      return compare.view.older ? `Rendering ${comparisonTitle(now).replace('Comparing ', '')}…` : 'Rendering the comparison…';
    }
    if (compare.mode) return comparisonTitle(now);
    const from = git.commits.find((commit) => commit.id === compare.marks.from);
    return from ? `${from.shortId} marked — click another commit to compare` : null;
  });

  function openRow(row: ChangeRow) {
    if (row.kind !== 'deleted') void openFile(row.path);
  }
</script>

{#snippet row(item: ChangeRow, staged: boolean)}
  <li class="change-row">
    <button class="change-open" title={item.renamedFrom ? `Renamed from ${item.renamedFrom}` : item.path} onclick={() => openRow(item)}>
      <span class="change-name" class:gone={item.kind === 'deleted'}>{item.name}</span>
      {#if item.dir}<span class="change-dir">{item.dir}</span>{/if}
    </button>
    <span class="change-actions">
      {#if staged}
        <button class="ghost" title="Unstage this file" onclick={() => void unstageChange(item.path)}>−</button>
      {:else}
        <button class="ghost" title="Discard changes to this file" onclick={() => void discardChange(item.path, item.kind === 'untracked')}>
          ↺
        </button>
        <button class="ghost" title="Stage this file" onclick={() => void stageChange(item.path)}>+</button>
      {/if}
    </span>
    <span class={`change-letter kind-${item.kind}`} title={item.kind}>{item.letter}</span>
  </li>
{/snippet}

{#snippet section(title: string, rows: ChangeRow[], staged: boolean)}
  {#if rows.length > 0}
    <div class="sc-section">
      <div class="sidebar-head">
        <span class="label">{title}</span>
        <span class="count">{rows.length}</span>
      </div>
      <ul class="change-list">
        {#each rows as item (item.path)}
          {@render row(item, staged)}
        {/each}
      </ul>
    </div>
  {/if}
{/snippet}

{#snippet snapshotsSection()}
  <!-- S11.6 (design interview B3): the safety net, in words. A snapshot is taken after every
       successful build, so an author who has never once pressed commit still has a list of
       versions to go back to. Shown with or without a Git repository: those are the authors it is
       for. -->
  <div class="sc-section">
    <div class="sidebar-head">
      <span class="label">Snapshots</span>
    </div>
    {#if snapshots.listError}
      <p class="hint error">{snapshots.listError}</p>
    {:else if snapshots.rows === null}
      <p class="hint">Reading…</p>
    {:else if snapshots.rows.length === 0}
      <p class="hint">Nothing yet. A version is kept every time the document builds.</p>
    {:else}
      <ul class="commit-list">
        {#each snapshots.rows as row (row.id)}
          <li class="commit-row">
            <button class="commit-button" onclick={() => void openSnapshot(row)} title="Look at this version">
              <div class="commit-summary">{snapshotLabel(row, now)}</div>
              <div class="commit-meta"><span class="short-id">{row.shortId}</span></div>
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </div>
{/snippet}

{#snippet signIn()}
  <!-- S10.4b: GitHub's device flow. The code is the whole interface — big enough to read off a
       screen, with one button that copies it and one that opens the browser. Nothing here is
       required to write a paper: a project with no remote works completely (rule 6). -->
  {#if github.stage === 'waiting' && github.code}
    <div class="sign-in">
      <p class="hint">Type this code into GitHub to finish signing in:</p>
      <div class="code-row">
        <code class="user-code">{github.code.userCode}</code>
        <button class="ghost" onclick={() => void copyCode()}>{copied ? 'Copied' : 'Copy'}</button>
      </div>
      <button class="primary" onclick={() => void openVerificationPage()}>Open GitHub</button>
      <!-- Written out as well as opened: a machine where the opener fails is not a machine where
           signing in is impossible. -->
      <p class="hint">
        {github.code.verificationUri} · {timeLeft(github.code.expiresAt, now)}
      </p>
      <button class="ghost" onclick={() => void cancelGitHubSignIn()}>Cancel</button>
    </div>
  {:else if github.account}
    <p class="hint">
      Signed in to GitHub as {github.account.login}
      <button class="ghost" onclick={() => void signOutOfGitHub()}>Sign out</button>
    </p>
  {:else if github.account === null}
    <div class="sign-in">
      <button class="ghost" disabled={github.stage === 'starting'} onclick={() => void signInToGitHub()}>
        {github.stage === 'starting' ? 'Asking GitHub…' : 'Sign in to GitHub…'}
      </button>
      {#if github.error}<p class="hint error">{github.error}</p>{/if}
    </div>
  {/if}
{/snippet}

{#snippet publish()}
  <!-- S10.5b: creating the repository on GitHub. Offered only where it means something — a
       project that is already a repository, signed in, with no remote yet. Private is not a
       default in a settings file: it is what the app sends unless the confirmation in `ipc.ts`
       has been answered yes. -->
  {#if github.created}
    <p class="hint">
      Created {github.created.fullName}{github.created.private ? ' (private)' : ' (public)'} and set
      it as this project's origin. Sending your work there arrives with Sync, in a later version.
    </p>
  {:else if git.isRepository && github.account && git.originUrl === null}
    <div class="sign-in">
      <p class="hint">This project has no remote yet — a copy on GitHub is what makes it a backup.</p>
      <input class="remote-name" bind:value={remoteName} aria-label="Repository name" />
      <label class="visibility">
        <input type="checkbox" bind:checked={wantPublic} />
        Make it public
      </label>
      <button
        class="primary"
        disabled={github.creating || remoteName.trim().length === 0}
        onclick={() => void createGitHubRepository(remoteName, wantPublic)}
      >
        {github.creating ? 'Creating…' : wantPublic ? 'Create a public repository' : 'Create a private repository'}
      </button>
      {#if github.createError}<p class="hint error">{github.createError}</p>{/if}
    </div>
  {:else if git.originUrl}
    <p class="hint">Remote: {git.originUrl}</p>
  {/if}
{/snippet}

<aside class="sidebar source-control">
  {#if !app.project}
    <p class="hint">Open a folder to see its changes.</p>
  {:else if !git.isRepository}
    <!-- S10.5a: the sentence S10.3a left here is a button now. What it does not do yet is the
         remote — that needs an account, and S10.5b. -->
    <p class="hint">This folder is not a Git repository, so there is no history to show.</p>
    <div class="sign-in">
      <button class="primary" disabled={git.initialising} onclick={() => void initialiseRepository()}>
        {git.initialising ? 'Creating…' : 'Create a Git repository here'}
      </button>
      <p class="hint">
        Adds a <code>.gitignore</code> for the build folder and makes a first commit of what is
        here. Nothing leaves this machine.
      </p>
    </div>
    {#if git.error}<p class="hint error">{git.error}</p>{/if}
    {#if git.readError}<p class="hint error">{git.readError}</p>{/if}
    {@render signIn()}
    {@render snapshotsSection()}
  {:else}
    <div class="sidebar-head">
      <span class="label">Source Control</span>
      <button class="ghost" title="Commit (Ctrl Enter)" disabled={!canCommit} onclick={() => void commitStaged()}>✓</button>
      <button class="ghost" title="Refresh" onclick={() => void refreshGitStatus()}>⟳</button>
    </div>

    <div class="commit-box">
      <textarea
        rows="2"
        placeholder={`Message (Ctrl Enter to commit${git.branch?.name ? ` on ${git.branch.name}` : ''})`}
        bind:value={git.message}
        oninput={claimCommitMessage}
        onkeydown={onMessageKeydown}
      ></textarea>
      <div class="commit-split">
        <button class="primary commit-main" disabled={!canCommit} onclick={() => void commitStaged()}>
          {git.committing ? 'Committing…' : 'Commit'}
        </button>
        <button
          class="primary commit-caret"
          disabled={!canOpenCommitMenu}
          aria-label="More commit actions"
          aria-haspopup="menu"
          aria-expanded={commitMenuOpen}
          onclick={() => (commitMenuOpen = !commitMenuOpen)}
        >
          ▾
        </button>
        {#if commitMenuOpen}
          <ul class="commit-menu" role="menu">
            <li role="none">
              <button role="menuitem" disabled={!canCommit} onclick={() => runCommitMenuItem(commitAndPush)}>
                Commit &amp; Push
              </button>
            </li>
            <li role="none">
              <button role="menuitem" disabled={!canCommit} onclick={() => runCommitMenuItem(commitAndSync)}>
                Commit &amp; Sync
              </button>
            </li>
            {#if git.showAmend}
              <!-- S11.1c: hidden rather than disabled once HEAD is already pushed — the design's
                   own guardrail, and the reason this item is not simply always here. -->
              <li role="none">
                <button role="menuitem" disabled={!canAmendNow} onclick={() => runCommitMenuItem(amendCommit)}>Amend</button>
              </li>
            {/if}
          </ul>
        {/if}
      </div>
      {#if git.wordsSinceCommit !== 0}
        <!-- S10.3c: the one number a writer checks, kept visible even after they have replaced
             our sentence with their own. -->
        <span class="prose-count">{wordDeltaLabel(git.wordsSinceCommit)} since the last commit</span>
      {/if}
    </div>

    {#if git.showSync}
      <!-- S11.1b: DESIGN.md §5.7's one-verb path, on its own — whenever the branch is ahead,
           behind, or both. Hidden the moment neither is true, the same test `syncArrows` already
           passes for its own empty string. -->
      <div class="sync-row">
        <button class="ghost sync-button" disabled={git.syncing || git.committing} onclick={() => void syncChanges()}>
          {git.syncing ? 'Syncing…' : `Sync Changes ${syncArrows(git.branch)}`}
        </button>
      </div>
    {/if}
    {#if git.lastSync}<p class="hint">{git.lastSync}</p>{/if}

    {#if git.readError}<p class="hint error">{git.readError}</p>{/if}
    {#if git.error}<p class="hint error">{git.error}</p>{/if}
    {#if git.lastDiscard}<p class="hint">{git.lastDiscard}</p>{/if}
    {#if git.needsIdentity}
      <!-- S10.5a: the repository is made and everything is staged; the first commit is theirs to
           make, because this app does not sign a history with a stand-in name (S10.2b). -->
      <p class="hint">
        The repository is ready and everything is staged. Git does not know who you are yet — set
        <code>git config --global user.name</code> and <code>user.email</code>, then commit.
      </p>
    {/if}
    {#if git.ourFolderIsIgnored === false}
      <!-- S10.2a's outcome note coming due: a repository older than this app has no line for our
           folder, and this offers to add one rather than editing their file unasked. -->
      <p class="hint">
        This repository does not ignore <code>.abstract-tex/</code> yet.
        <button class="ghost" onclick={() => void ignoreOurFolder()}>Add it to .gitignore</button>
      </p>
    {/if}

    {@render signIn()}
    {@render publish()}

    {#if git.conflictedRows.length > 0}
      <!-- A conflicted path is in no other list, so without this section it would vanish from
           the one panel whose job is to say what changed. Clicking a row opens the conflict view
           (S11.2b) instead of the plain editor, the same way `openRow` already opens any other
           file — nothing here has to know that. -->
      {@render section('Merge Changes', git.conflictedRows, false)}
      <p class="hint">Open a file to resolve it as two paragraphs — resolving it stages it.</p>
    {/if}
    {@render section('Staged Changes', git.stagedRows, true)}

    {#if git.visibleLargeFiles.length > 0}
      <!-- S11.3c, the maintainer's own shape for this: a dismissible banner above Changes, never
           a per-row badge or a dialog that would interrupt Stage — Stage has never asked a
           question before, and this is not the loop that teaches it to. -->
      <div class="lfs-banner">
        <p class="hint">
          {#if git.visibleLargeFiles.length === 1 && git.visibleLargeFiles[0]}
            {@const only = git.visibleLargeFiles[0]}
            {only.path} is {formatMegabytes(only.sizeBytes)}.
          {:else}
            {git.visibleLargeFiles.length} files are 5 MB or larger.
          {/if}
          Git stores every version of a binary file in full, which makes the repository grow fast.
        </p>
        <div class="lfs-actions">
          <button
            class="ghost"
            disabled={git.trackingLfs}
            onclick={() => void trackLargeFilesWithLfs(git.visibleLargeFiles.map((file) => file.path))}
          >
            {git.trackingLfs ? 'Tracking…' : 'Track with Git LFS'}
          </button>
          <button
            class="ghost"
            disabled={git.trackingLfs}
            onclick={() => dismissLargeFiles(git.visibleLargeFiles.map((file) => file.path))}
          >
            Dismiss
          </button>
        </div>
      </div>
    {/if}
    {@render section('Changes', git.unstagedRows, false)}

    {#if git.changedCount === 0}
      <p class="hint">No changes. Everything here matches the last commit.</p>
    {/if}

    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="sc-section" onkeydown={onGraphKeydown}>
      <div class="sidebar-head">
        <span class="label">Graph</span>
      </div>
      <!-- S11.4d: click one commit, then another, to read what changed between them. -->
      {#if toolbarText}
        <div class="compare-bar" role="status">
          <span class="compare-text">{toolbarText}</span>
          <button class="ghost" title="Clear (Esc)" aria-label="Clear the comparison" onclick={clearComparison}>×</button>
        </div>
      {/if}
      {#if compare.refusal}<p class="hint error">{compare.refusal}</p>{/if}
      {#if git.branch?.unborn}
        <p class="hint">No commits yet. The first one starts the history.</p>
      {:else}
        <ul class="commit-list">
          {#each git.commits as commit, index (commit.id)}
            {#if git.outgoing > 0 && index === 0}
              <!-- The newest `outgoing` rows are the ones the upstream does not have. One number
                   is enough to say where the line goes, which is what VS Code does too. -->
              <li class="graph-head">Outgoing changes</li>
            {/if}
            {#if git.outgoing > 0 && index === git.outgoing}
              <li class="graph-head">On {git.branch?.name ?? 'the remote'}</li>
            {/if}
            {@const mark = markOf(compare.marks, commit.id)}
            <li class="commit-row" class:marked={mark !== null}>
              <!-- A real button, so Tab reaches it and Enter/Space mark exactly as a click does
                   (rule 5). -->
              <button
                class="commit-button"
                aria-pressed={mark !== null}
                title={mark === 'from'
                  ? 'Marked as the start of a comparison. Click to unmark.'
                  : mark === 'to'
                    ? 'Marked as the end of a comparison. Click to unmark.'
                    : compare.marks.from === null
                      ? 'Click to start a comparison here'
                      : 'Click to compare with the marked commit'}
                onclick={() => markGraphRow(commit.id)}
              >
                <div class="commit-summary">
                  {#if mark}<span class="mark-badge">{mark === 'from' ? 'from' : 'to'}</span>{/if}
                  {commit.summary || '(no message)'}
                  {#each commit.tags as tag (tag)}<span class="tag">{tag}</span>{/each}
                </div>
                <div class="commit-meta">
                  <span class="short-id">{commit.shortId}</span>
                  {commit.author} · {relativeTime(commit.time, now)}
                  {#if commit.wordDelta !== 0}
                    <!-- S10.3c: this is what makes the graph double as a progress log. -->
                    <span class={commit.wordDelta > 0 ? 'ok' : 'warn'}>· {wordDeltaLabel(commit.wordDelta)}</span>
                  {/if}
                </div>
              </button>
            </li>
          {/each}
        </ul>
        {#if git.mayHaveMore}
          <button class="ghost more" onclick={() => void loadMoreCommits()}>Show more</button>
        {/if}
      {/if}
    </div>
    {@render snapshotsSection()}
  {/if}
</aside>

<style>
  .source-control {
    overflow-y: auto;
  }
  .sidebar-head {
    display: flex;
    align-items: center;
    padding: 2px 6px 6px 10px;
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--fg-muted);
  }
  .sidebar-head .label {
    flex: 1;
  }
  .sidebar-head .count {
    padding: 0 4px;
  }
  .hint {
    color: var(--fg-muted);
    padding: 4px 12px 8px;
    margin: 0;
    font-size: 12px;
    line-height: 1.45;
  }
  .hint.error {
    color: var(--error);
  }
  .sc-section {
    margin-bottom: 6px;
  }
  .change-list {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .change-row {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 0 6px 0 10px;
  }
  .change-row:hover {
    background: var(--bg-hover);
  }
  .change-open {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: baseline;
    gap: 6px;
    background: none;
    border: 0;
    color: inherit;
    font: inherit;
    text-align: left;
    padding: 3px 0;
    cursor: pointer;
  }
  .change-name {
    white-space: nowrap;
  }
  /* A deleted file is still worth naming, but it is not there to be opened. */
  .change-name.gone {
    text-decoration: line-through;
  }
  .change-dir {
    color: var(--fg-muted);
    font-size: 11px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  /* Hover actions, as VS Code has them: present in the layout only when the row is hovered or
     one of its own buttons has focus, so a keyboard user can still reach them. */
  .change-actions {
    display: flex;
    gap: 2px;
    visibility: hidden;
  }
  .change-row:hover .change-actions,
  .change-actions:focus-within {
    visibility: visible;
  }
  .change-actions button {
    padding: 0 4px;
    line-height: 1.2;
  }
  .change-letter {
    width: 1.2em;
    text-align: center;
    font-size: 11px;
    font-weight: 600;
    color: var(--fg-muted);
  }
  .kind-modified,
  .kind-renamed {
    color: var(--warn);
  }
  .kind-added,
  .kind-untracked {
    color: var(--ok);
  }
  .kind-deleted,
  .kind-conflicted {
    color: var(--error);
  }

  .commit-box {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 0 10px 8px;
  }
  .commit-box textarea {
    resize: vertical;
    font: inherit;
    padding: 4px 6px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg-editor);
    color: var(--fg);
  }
  /* The Commit button and its dropdown caret read as one control, VS Code's own split-button
     shape: a shared border with the seam between them the only hint there are two. */
  .commit-split {
    position: relative;
    display: flex;
  }
  .commit-main {
    flex: 1;
    border-top-right-radius: 0;
    border-bottom-right-radius: 0;
  }
  .commit-caret {
    border-top-left-radius: 0;
    border-bottom-left-radius: 0;
    border-left-color: var(--accent-fg);
    padding: 3px 8px;
  }
  .commit-menu {
    position: absolute;
    bottom: calc(100% + 4px);
    right: 0;
    z-index: 1;
    margin: 0;
    padding: 4px;
    list-style: none;
    min-width: 160px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg-panel);
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.2);
  }
  .commit-menu button {
    display: block;
    width: 100%;
    text-align: left;
    border: 0;
    padding: 5px 8px;
  }
  .sync-row {
    padding: 0 10px 8px;
  }
  .sync-button {
    width: 100%;
  }
  /* S11.3c: advisory, not an error — `--warn`, the same border `ConflictResolver`'s own box uses
     rather than `--error`'s red, since nothing here is wrong yet. */
  .lfs-banner {
    margin: 0 10px 8px;
    padding: 8px;
    border: 1px solid var(--warn);
    border-radius: var(--radius);
  }
  .lfs-banner .hint {
    padding: 0 0 6px;
  }
  .lfs-actions {
    display: flex;
    gap: 6px;
  }
  .commit-list {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .commit-row {
    padding: 0;
  }
  .commit-row:hover {
    background: var(--bg-hover);
  }
  .commit-row.marked {
    background: var(--bg-hover);
    box-shadow: inset 2px 0 0 var(--accent);
  }
  /* The whole row is the button; it only has to look like the row it replaced. */
  .commit-button {
    display: block;
    width: 100%;
    padding: 3px 10px;
    border: 0;
    background: transparent;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .mark-badge {
    margin-right: 5px;
    padding: 0 5px;
    border-radius: 8px;
    background: var(--accent);
    color: var(--bg-editor);
    font-size: 10px;
    text-transform: uppercase;
  }
  .compare-bar {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 2px 6px 4px 10px;
    font-size: 12px;
  }
  .compare-text {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .commit-summary {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .commit-meta {
    font-size: 11px;
    color: var(--fg-muted);
  }
  .sign-in {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 4px;
    padding: 0 10px 8px;
  }
  .code-row {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  /* Big enough to read off a screen and type into a phone, which is what it is for. */
  .user-code {
    font-family: var(--font-mono);
    font-size: 18px;
    letter-spacing: 0.12em;
    padding: 2px 6px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg-editor);
  }
  .remote-name {
    font: inherit;
    padding: 3px 6px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg-editor);
    color: var(--fg);
    width: 100%;
  }
  .visibility {
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: 12px;
    color: var(--fg-muted);
  }
  .prose-count {
    font-size: 11px;
    color: var(--fg-muted);
  }
  .commit-meta .ok {
    color: var(--ok);
  }
  .commit-meta .warn {
    color: var(--warn);
  }
  .short-id {
    font-family: var(--font-mono);
    margin-right: 5px;
  }
  .tag {
    margin-left: 5px;
    padding: 0 5px;
    border: 1px solid var(--border);
    border-radius: 8px;
    font-size: 10px;
    color: var(--fg-muted);
  }
  /* Not a section head: it sits between rows, marking where the upstream's history begins. */
  .graph-head {
    padding: 6px 10px 2px;
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--fg-muted);
  }
  .more {
    margin: 4px 10px 10px;
  }
</style>
