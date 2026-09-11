<script lang="ts">
  import type { TreeNode } from '../lib/ipc';
  import TreeItem from './TreeItem.svelte';

  interface Props {
    node: TreeNode;
    depth: number;
    activePath: string | null;
    rootFile: string | null;
    /** Paths with edits not yet on disk (S2.3) — any open tab, not only the active one. */
    dirtyPaths: Set<string>;
    onOpen: (path: string) => void;
    onSetRoot: (path: string) => void;
  }
  let { node, depth, activePath, rootFile, dirtyPaths, onOpen, onSetRoot }: Props = $props();

  // Folders start open so a small project is visible at a glance. Only the initial depth
  // matters, which is what the svelte-ignore below is about.
  // svelte-ignore state_referenced_locally
  let expanded = $state(depth < 2);

  const isActive = $derived(node.path === activePath);
  const isRoot = $derived(node.path === rootFile);
  const isTex = $derived(/\.tex$/i.test(node.name));
  const isDirty = $derived(dirtyPaths.has(node.path));

  function activate() {
    if (node.isDir) expanded = !expanded;
    else onOpen(node.path);
  }

  function onKey(event: KeyboardEvent) {
    if (event.key === 'Enter' || event.key === ' ') {
      event.preventDefault();
      activate();
    }
  }
</script>

<li>
  <div
    class="row"
    class:active={isActive}
    class:dirty={isDirty}
    style:padding-left="{8 + depth * 14}px"
    role="treeitem"
    tabindex="0"
    aria-selected={isActive}
    aria-expanded={node.isDir ? expanded : undefined}
    onclick={activate}
    onkeydown={onKey}
    ondblclick={() => isTex && !isRoot && onSetRoot(node.path)}
    title={isTex && !isRoot ? 'Double-click to make this the root file' : node.path}
  >
    <span class="chev">{node.isDir ? (expanded ? '▾' : '▸') : ''}</span>
    <span class="name">{node.name}</span>
    {#if isRoot}<span class="root-badge">root</span>{/if}
  </div>
  {#if node.isDir && expanded && node.children.length > 0}
    <ul class="tree" role="group">
      {#each node.children as child (child.path)}
        <TreeItem node={child} depth={depth + 1} {activePath} {rootFile} {dirtyPaths} {onOpen} {onSetRoot} />
      {/each}
    </ul>
  {/if}
</li>
