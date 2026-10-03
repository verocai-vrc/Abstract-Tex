// S2.1 end to end: a file changes on disk, the watcher event reaches the controller, and the
// change lands in the CRDT — or stops at a question. The IPC layer is faked, so this exercises
// every line of the reaction except the Rust on the far side of `invoke`.

import { beforeEach, afterEach, describe, expect, it, vi } from 'vitest';
import type {
  BibliographyIndex,
  BranchState,
  CommitRow,
  CompileEvent,
  ComparisonStarted,
  GitHubRepository,
  SnapshotRow,
  DiffSides,
  Discarded,
  Finding,
  ProseSummary,
  SignInEvent,
  FsEvent,
  GitStatus,
  LargeFile,
  LspEvent,
  ProjectInfo,
  SyncOutcome,
  TextOp,
} from './ipc';

/** The fake disk and the calls made against it. Declared before the mock factory uses it. */
const disk = new Map<string, string>();
const calls = {
  compiles: 0,
  /** The `file` each compile was asked to draft a chapter of (S9.9). */
  compileFiles: [] as Array<string | null>,
  /** The `draft` flag of each SyncTeX query, forward or inverse (S9.9). */
  synctexDraft: [] as boolean[],
  /** How often the shell-escape question was put to the person (S9.8). */
  shellEscapeQuestions: 0,
  writes: [] as Array<{ path: string; contents: string }>,
  reads: [] as string[],
  /** Every `textDocument/*` message the controller sent the language server. */
  lsp: [] as Array<{ method: string; params: any }>,
  lspStarts: 0,
  /** How often the raw view asked for `main.log` (S6.3). */
  logReads: 0,
  /** How often the Source Control view asked Rust what changed (S10.3a). */
  gitStatusReads: 0,
  /** Every Git verb called, as `'<verb> <path>'`, and every discard the author was asked about. */
  gitVerbs: [] as string[],
  discardQuestions: [] as string[],
  /** Every page of history asked for, as `'<skip>+<limit>'` (S10.3b). */
  gitLogPages: [] as string[],
  /** Every GitHub call made, as its verb (S10.4b), and every URL handed to the browser. */
  githubCalls: [] as string[],
  openedUrls: [] as string[],
  /** Every repository creation asked for, as `'<name> <private|public>'` (S10.5b), and every
   * time the public confirmation was put to the person. */
  created: [] as string[],
  publicQuestions: [] as string[],
};
/** This machine's shell-escape consent for `/proj` (S9.8), and what the person answers when asked. */
let shellEscapeOnDisk = false;
let shellEscapeAnswer = false;
/** Set to a message to make `lsp_start` fail, as a machine with no TexLab would. */
let lspStartError: string | null = null;
/** What `synctexForward`/`synctexInverse` answer with, or throw, for the S3.4/S3.5 tests below. */
let synctexForwardAnswer: { page: number; x: number; y: number } | Error = { page: 1, x: 10, y: 20 };
let synctexInverseAnswer: { file: string | null; line: number } | Error = { file: 'main.tex', line: 3 };
/** What `lspRequest` answers with next, for the hover/definition/documentSymbol tests below.
 * `unknown` because that is genuinely what crosses the Tauri event boundary; each test narrows
 * it to whatever shape it is pretending TexLab sent. */
let lspRequestAnswer: unknown = null;
/** What `readLog` returns — the `main.log` on the fake disk, for the raw-view tests (S6.3). */
let logOnDisk = '';
/** What `bibliographyIndex` answers with — the index Rust would have built from the fake disk. */
const emptyBibliography: BibliographyIndex = { files: [], entries: [], citations: [], hasNociteStar: false };
let bibliographyOnDisk: BibliographyIndex = emptyBibliography;
let bibliographyHandler: (index: BibliographyIndex) => void = () => {};
/** The fake repository the Source Control tests (S10.3a) read and act on. `null` stands for a
 * folder that is not inside a Git repository at all, which is a sentence in the view. */
let gitOnDisk: GitStatus | null = { staged: [], unstaged: [], conflicted: [] };
/** What the author answers when asked to confirm a discard. */
let discardAnswer = false;
/** Set to a message to make the next Git verb fail, as Rust would for a path outside the
 * project or a repository it cannot write. */
let gitVerbError: string | null = null;
/** The fake history, newest first, and the branch it is on (S10.3b). */
let commitsOnDisk: CommitRow[] = [];
let branchOnDisk: BranchState = { name: 'main', aheadBehind: null, unborn: false, head: 'head0' };
/** What `gitProseSummary` answers with (S10.3c). */
let proseOnDisk: ProseSummary = { wordsBefore: 0, wordsAfter: 0, sections: [], paths: [], addedPaths: [] };
/** Set to a message to make the next commit be refused, as the crate refuses an empty message,
 * an empty stage or a repository with no `user.name`. */
let commitError: string | null = null;
/** What the next `git_sync` answers with (S11.1b), and a message to make `git_push`/`git_sync`
 * fail — the server-side refusal or the stale-branch one, `abstract-tex-git`'s own tests tell
 * apart; this layer only has to carry whichever sentence Rust would have sent. */
let syncOutcome: SyncOutcome = { kind: 'upToDate' };
let syncError: string | null = null;
/** S11.3c: the LFS banner's own candidates, and a message to make `git_track_with_lfs` fail —
 * most often "Git LFS isn't installed", which is a sentence and not a thrown shape this fake
 * needs to distinguish from any other verb's refusal. */
let largeFilesOnDisk: LargeFile[] = [];
let lfsError: string | null = null;
/** The GitHub side (S10.4b): who the keychain says is signed in, what `github_sign_in` does, and
 * the handler the controller registered for `github:sign-in`. */
/** S10.5a: whether the fake project is a repository, and whether it ignores our folder. */
let initialiseError: string | null = null;
let initialiseNeedsIdentity = false;
let ourFolderIgnoredOnDisk: boolean | null = true;
let accountOnDisk: { login: string } | null = null;
let signInError: string | null = null;
let signInHandler: (event: SignInEvent) => void = () => {};
/** S10.5b: what the author answers to the public-repository confirmation, what the fake project's
 * `origin` is, and what `github_create_repository` was asked for. */
let publicAnswer = false;
let originOnDisk: string | null = null;
let createError: string | null = null;
let gitStatusHandler: () => void = () => {};
let fsHandler: (event: FsEvent) => void = () => {};
let compileHandler: (event: CompileEvent) => void = () => {};
/** S11.4d: the comparison lane's own event handler, what `compare_revisions` answers with (a
 * function, so a test can hold an answer back and let a newer one overtake it), and every pair
 * it was asked for and every place a PDF was saved. */
let diffHandler: (event: CompileEvent) => void = () => {};
let compareAnswer: (older: string, newer: string) => Promise<ComparisonStarted> = async (a, b) => ({
  status: 'building',
  older: a,
  newer: b,
  generation: 1,
});
let savePathAnswer: string | null = '/out/diff.pdf';
/** S11.5b: what the Clone window's calls answer with, and what they were asked. */
let listAnswer: GitHubRepository[] | Error = [];
let cloneAnswer: string | Error = '/papers/thesis';
let parentFolderAnswer: string | null = '/papers';
/** S11.6: the fake snapshot ref's rows, each version's files and text, and every restore asked for. */
let snapshotRows: SnapshotRow[] = [];
let snapshotFilesOnDisk: Record<string, Record<string, string>> = {};
let snapshotError: string | null = null;
const restores: Array<[string, string]> = [];
/** S11.7: what `gitDiffSides` answers with, or throws, and every pair it was asked for. */
let diffAnswer: DiffSides | Error = { before: 'a', after: 'b' };
const diffsAsked: Array<[string, boolean]> = [];
const clones = { asked: [] as Array<[string, string, string | null]>, lists: 0 };
const comparisons = { asked: [] as Array<[string, string]>, saved: [] as string[] };
let lspHandler: (event: LspEvent) => void = () => {};

const project: ProjectInfo = {
  rootDir: '/proj',
  rootFile: 'main.tex',
  buildDir: '/proj/.abstract-tex/build',
  tree: [],
  // Every test that triggers a filesystem event uses main.tex (see `fileChanged` below), so a
  // complete graph that lists it is enough to keep `shouldCompileFor` compiling as these tests
  // expect, without every test needing to know about S4.1's include graph.
  documentFiles: ['main.tex'],
  documentFilesComplete: true,
  engineNotice: null,
};

// `vi.mock` with a factory means the real ./ipc — and with it @tauri-apps/api — is never
// imported, so these tests run under plain Node with no webview.
vi.mock('./ipc', () => ({
  ipc: {
    initialProject: async () => null,
    engineInfo: async () => ({ name: 'Tectonic', version: '0.17.0', path: '/bin/tectonic' }),
    openProject: async () => project,
    shellEscapeAllowed: async () => shellEscapeOnDisk,
    allowShellEscape: async () => {
      shellEscapeOnDisk = true;
    },
    disallowShellEscape: async () => {
      shellEscapeOnDisk = false;
    },
    confirmShellEscape: async () => {
      calls.shellEscapeQuestions++;
      return shellEscapeAnswer;
    },
    refreshTree: async () => project,
    readFile: async (path: string) => {
      calls.reads.push(path);
      const text = disk.get(path);
      if (text === undefined) throw new Error(`no such file: ${path}`);
      return text;
    },
    writeFile: async (path: string, contents: string) => {
      calls.writes.push({ path, contents });
      disk.set(path, contents);
    },
    compile: async (file: string | null) => {
      calls.compileFiles.push(file);
      return ++calls.compiles;
    },
    diffOps: async (oldText: string, newText: string): Promise<TextOp[]> => {
      // Prefix/suffix trimming, as the Rust reconciler does. Its own correctness is proved by
      // the proptests in abstract-tex-reconcile; here it only has to leave untouched text alone.
      let prefix = 0;
      while (prefix < oldText.length && prefix < newText.length && oldText[prefix] === newText[prefix]) prefix++;
      let suffix = 0;
      while (
        suffix < oldText.length - prefix &&
        suffix < newText.length - prefix &&
        oldText[oldText.length - 1 - suffix] === newText[newText.length - 1 - suffix]
      )
        suffix++;
      return [
        { index: prefix, delete: oldText.length - prefix - suffix, insert: newText.slice(prefix, newText.length - suffix) },
      ];
    },
    readLog: async () => {
      calls.logReads++;
      return logOnDisk;
    },
    assetUrl: (p: string) => `asset://${p}`,
    synctexForward: async (_file: string, _line: number, draft: boolean) => {
      calls.synctexDraft.push(draft);
      if (synctexForwardAnswer instanceof Error) throw synctexForwardAnswer;
      return synctexForwardAnswer;
    },
    synctexInverse: async (_page: number, _x: number, _y: number, draft: boolean) => {
      calls.synctexDraft.push(draft);
      if (synctexInverseAnswer instanceof Error) throw synctexInverseAnswer;
      return synctexInverseAnswer;
    },
    onCompile: async (handler: (event: CompileEvent) => void) => {
      compileHandler = handler;
      return () => {};
    },
    onCompileDiff: async (handler: (event: CompileEvent) => void) => {
      diffHandler = handler;
      return () => {};
    },
    compareRevisions: async (a: string, b: string) => {
      comparisons.asked.push([a, b]);
      return compareAnswer(a, b);
    },
    pickSavePath: async () => savePathAnswer,
    gitDiffSides: async (path: string, staged: boolean) => {
      diffsAsked.push([path, staged]);
      if (diffAnswer instanceof Error) throw diffAnswer;
      return diffAnswer;
    },
    snapshotList: async () => {
      if (snapshotError) throw new Error(snapshotError);
      return snapshotRows;
    },
    snapshotFiles: async (id: string) => Object.keys(snapshotFilesOnDisk[id] ?? {}),
    snapshotRead: async (id: string, path: string) => {
      const text = snapshotFilesOnDisk[id]?.[path];
      if (text === undefined) throw new Error(`That version of the project has no ${path}.`);
      return text;
    },
    snapshotRestore: async (id: string, path: string) => {
      if (snapshotError) throw new Error(snapshotError);
      restores.push([id, path]);
    },
    githubListRepositories: async () => {
      clones.lists++;
      if (listAnswer instanceof Error) throw listAnswer;
      return listAnswer;
    },
    pickParentFolder: async () => parentFolderAnswer,
    gitClone: async (url: string, parent: string, name: string | null) => {
      clones.asked.push([url, parent, name]);
      if (cloneAnswer instanceof Error) throw cloneAnswer;
      return cloneAnswer;
    },
    saveComparisonPdf: async (_older: string, _newer: string, destination: string) => {
      comparisons.saved.push(destination);
    },
    onFsChanged: async (handler: (event: FsEvent) => void) => {
      fsHandler = handler;
      return () => {};
    },
    lspStart: async () => {
      calls.lspStarts++;
      if (lspStartError) throw new Error(lspStartError);
      return {};
    },
    lspRequest: async () => lspRequestAnswer,
    lspNotify: async (method: string, params: unknown) => {
      calls.lsp.push({ method, params });
    },
    lspRespond: async () => {},
    onLsp: async (handler: (event: LspEvent) => void) => {
      lspHandler = handler;
      return () => {};
    },
    gitStatus: async (): Promise<GitStatus | null> => {
      calls.gitStatusReads++;
      return gitOnDisk;
    },
    gitStage: async (path: string) => {
      if (gitVerbError) throw new Error(gitVerbError);
      calls.gitVerbs.push(`stage ${path}`);
      const moving = gitOnDisk!.unstaged.filter((c) => c.path === path);
      // S11.2a: staging a conflicted path is what resolves it — the three index stages become
      // one, same as the real crate. A path that was conflicted lands in `staged` even with no
      // matching `unstaged` row, the same way an edited-and-fixed file would.
      const wasConflicted = gitOnDisk!.conflicted.some((c) => c.path === path);
      const resolved = wasConflicted && moving.length === 0 ? [{ path, kind: 'modified' as const, renamedFrom: null }] : [];
      gitOnDisk = {
        ...gitOnDisk!,
        staged: [...gitOnDisk!.staged, ...moving, ...resolved],
        unstaged: gitOnDisk!.unstaged.filter((c) => c.path !== path),
        conflicted: gitOnDisk!.conflicted.filter((c) => c.path !== path),
      };
      gitStatusHandler();
    },
    gitUnstage: async (path: string) => {
      calls.gitVerbs.push(`unstage ${path}`);
      gitStatusHandler();
    },
    gitDiscard: async (path: string): Promise<Discarded> => {
      calls.gitVerbs.push(`discard ${path}`);
      const untracked = gitOnDisk!.unstaged.some((c) => c.path === path && c.kind === 'untracked');
      gitOnDisk = { ...gitOnDisk!, unstaged: gitOnDisk!.unstaged.filter((c) => c.path !== path) };
      gitStatusHandler();
      return untracked ? 'deleted' : 'restored';
    },
    confirmDiscard: async (path: string, untracked: boolean) => {
      calls.discardQuestions.push(`${path}${untracked ? ' (untracked)' : ''}`);
      return discardAnswer;
    },
    gitInitialise: async () => {
      calls.gitVerbs.push('initialise');
      if (initialiseError) throw new Error(initialiseError);
      // A folder that was not a repository is one now, with its first commit.
      gitOnDisk = { staged: [], unstaged: [], conflicted: [] };
      ourFolderIgnoredOnDisk = true;
      return { firstCommit: initialiseNeedsIdentity ? null : 'commit1', needsIdentity: initialiseNeedsIdentity };
    },
    gitOurFolderIsIgnored: async (): Promise<boolean | null> => (gitOnDisk === null ? null : ourFolderIgnoredOnDisk),
    gitIgnoreOurFolder: async () => {
      calls.gitVerbs.push('ignoreOurFolder');
      ourFolderIgnoredOnDisk = true;
    },
    gitBranch: async (): Promise<BranchState | null> => (gitOnDisk === null ? null : branchOnDisk),
    gitProseSummary: async (): Promise<ProseSummary | null> => (gitOnDisk === null ? null : proseOnDisk),
    gitLog: async (skip: number, limit: number): Promise<CommitRow[]> => {
      calls.gitLogPages.push(`${skip}+${limit}`);
      return commitsOnDisk.slice(skip, skip + limit);
    },
    gitCommit: async (message: string): Promise<string> => {
      if (commitError) throw new Error(commitError);
      calls.gitVerbs.push(`commit ${message}`);
      const id = `commit${commitsOnDisk.length + 1}`;
      commitsOnDisk = [
        { id, shortId: id.slice(0, 7), summary: message.split('\n')[0]!, author: 'Ada', time: 1_760_000_000, tags: [], wordDelta: 7 },
        ...commitsOnDisk,
      ];
      gitOnDisk = { ...gitOnDisk!, staged: [] };
      // A commit moves HEAD, which is what tells the panel the history — and not just the
      // working tree — has changed (S10.3c).
      branchOnDisk = { ...branchOnDisk, head: id };
      gitStatusHandler();
      return id;
    },
    gitAmend: async (message: string): Promise<string> => {
      if (commitError) throw new Error(commitError);
      calls.gitVerbs.push(`amend ${message}`);
      // Rewrites the newest commit in place, the same way the crate keeps the same parent.
      const id = `${commitsOnDisk[0]?.id ?? 'commit1'}-amended`;
      commitsOnDisk = [{ ...commitsOnDisk[0]!, id, summary: message.split('\n')[0]! }, ...commitsOnDisk.slice(1)];
      gitOnDisk = { ...gitOnDisk!, staged: [] };
      branchOnDisk = { ...branchOnDisk, head: id };
      gitStatusHandler();
      return id;
    },
    gitPush: async (): Promise<void> => {
      if (syncError) throw new Error(syncError);
      calls.gitVerbs.push('push');
      // A push is always a plain fast-forward in this fake: whatever was ahead is not any more.
      branchOnDisk = { ...branchOnDisk, aheadBehind: branchOnDisk.aheadBehind ? [0, branchOnDisk.aheadBehind[1]] : null };
      gitStatusHandler();
    },
    gitSync: async (): Promise<SyncOutcome> => {
      if (syncError) throw new Error(syncError);
      calls.gitVerbs.push('sync');
      branchOnDisk = { ...branchOnDisk, aheadBehind: [0, 0] };
      gitStatusHandler();
      return syncOutcome;
    },
    gitLargeFiles: async (): Promise<LargeFile[] | null> => (gitOnDisk === null ? null : largeFilesOnDisk),
    gitTrackWithLfs: async (paths: string[]): Promise<void> => {
      if (lfsError) throw new Error(lfsError);
      calls.gitVerbs.push(`trackWithLfs ${paths.join(',')}`);
      largeFilesOnDisk = largeFilesOnDisk.filter((file) => !paths.includes(file.path));
      gitStatusHandler();
    },
    githubAccount: async () => {
      calls.githubCalls.push('account');
      return accountOnDisk;
    },
    githubSignIn: async () => {
      calls.githubCalls.push('signIn');
      if (signInError) throw new Error(signInError);
    },
    githubCancelSignIn: async () => {
      calls.githubCalls.push('cancel');
    },
    githubSignOut: async () => {
      calls.githubCalls.push('signOut');
      accountOnDisk = null;
    },
    githubCreateRepository: async (name: string, visibility: { kind: string }) => {
      calls.created.push(`${name} ${visibility.kind}`);
      if (createError) throw new Error(createError);
      const isPrivate = visibility.kind === 'private';
      originOnDisk = `https://github.com/ada/${name}.git`;
      return {
        fullName: `ada/${name}`,
        cloneUrl: originOnDisk,
        htmlUrl: `https://github.com/ada/${name}`,
        private: isPrivate,
      };
    },
    gitOriginUrl: async () => (gitOnDisk === null ? null : originOnDisk),
    confirmPublicRemote: async (name: string) => {
      calls.publicQuestions.push(name);
      return publicAnswer;
    },
    openInBrowser: async (url: string) => {
      calls.openedUrls.push(url);
    },
    onGitHubSignIn: async (handler: (event: SignInEvent) => void) => {
      signInHandler = handler;
      return () => {};
    },
    onGitStatusChanged: async (handler: () => void) => {
      gitStatusHandler = handler;
      return () => {};
    },
    bibliographyIndex: async (): Promise<BibliographyIndex> => bibliographyOnDisk,
    bibliographyHealth: async (): Promise<Finding[]> => [],
    onBibliographyChanged: async (handler: (index: BibliographyIndex) => void) => {
      bibliographyHandler = handler;
      return () => {};
    },
  },
}));

const {
  allowShellEscape,
  amendCommit,
  applyDiagnosticFix,
  claimCommitMessage,
  commitAndPush,
  commitAndSync,
  commitStaged,
  discardChange,
  dismissLargeFiles,
  syncChanges,
  loadMoreCommits,
  applyFindingFix,
  closeTab,
  disallowShellEscape,
  goToOutlineItem,
  jumpToDiagnostic,
  lspDiagnosticsFor,
  lspDocumentSymbols,
  lspGoToDefinition,
  lspHover,
  openFile,
  openFolder,
  quickOpenPick,
  refreshOutline,
  resolveConflict,
  resolveMergeConflict,
  cancelGitHubSignIn,
  createGitHubRepository,
  ignoreOurFolder,
  initialiseRepository,
  openVerificationPage,
  refreshGitStatus,
  setDrawerFilter,
  signInToGitHub,
  signOutOfGitHub,
  showActivityView,
  showRawLogFor,
  stageChange,
  start,
  unstageChange,
  syncTexForward,
  syncTexInverse,
  toggleDrawer,
  toggleQuickOpen,
  toggleRawLog,
  trackLargeFilesWithLfs,
  triggerCompile,
  clearComparison,
  compareWithPreviousCommit,
  markGraphRow,
  saveComparisonAs,
  chooseRepositoryToClone,
  cloneRepository,
  closeCloneWindow,
  loadCloneList,
  showCloneWindow,
  closeDiff,
  closeSnapshot,
  openChangeRow,
  openDiffedFile,
  openSnapshot,
  refreshSnapshots,
  restoreSnapshotFile,
  showSnapshotFile,
} = await import('./controller.svelte');
const { snapshots } = await import('./snapshots.svelte');
const { diffView } = await import('./diffview.svelte');
const { compare } = await import('./compare.svelte');
const { clone } = await import('./clone.svelte');
const { ipc } = await import('./ipc');
const { app } = await import('./state.svelte');
const { bibliography } = await import('./bibliography.svelte');
const { git } = await import('./git.svelte');
const { github } = await import('./github.svelte');
const { allCommands } = await import('./commands');

/** Pretend the watcher saw `path` change, and let the controller finish reacting. */
async function fileChanged(relative: string): Promise<void> {
  fsHandler({ path: `/proj/${relative}`, exists: disk.has(relative) });
  await vi.advanceTimersByTimeAsync(0);
}

/** Type into the open buffer as the author would, without waiting for the 700 ms save. */
function type(text: string): void {
  const doc = app.activeDoc!;
  doc.ytext.insert(doc.ytext.length, text);
}

/** Same, but into a specific tab rather than whichever one is active. */
function typeInto(path: string, text: string): void {
  const doc = app.docs.get(path)!;
  doc.ytext.insert(doc.ytext.length, text);
}

beforeEach(async () => {
  vi.useFakeTimers();
  disk.clear();
  disk.set('main.tex', 'hello');
  calls.compiles = 0;
  calls.compileFiles = [];
  calls.synctexDraft = [];
  calls.shellEscapeQuestions = 0;
  shellEscapeOnDisk = false;
  shellEscapeAnswer = false;
  calls.writes = [];
  calls.reads = [];
  calls.lsp = [];
  calls.lspStarts = 0;
  calls.logReads = 0;
  lspStartError = null;
  lspRequestAnswer = null;
  logOnDisk = '';
  synctexForwardAnswer = { page: 1, x: 10, y: 20 };
  synctexInverseAnswer = { file: 'main.tex', line: 3 };
  bibliographyOnDisk = emptyBibliography;
  gitOnDisk = { staged: [], unstaged: [], conflicted: [] };
  discardAnswer = false;
  gitVerbError = null;
  commitsOnDisk = [];
  branchOnDisk = { name: 'main', aheadBehind: null, unborn: false, head: 'head0' };
  proseOnDisk = { wordsBefore: 0, wordsAfter: 0, sections: [], paths: [], addedPaths: [] };
  commitError = null;
  syncOutcome = { kind: 'upToDate' };
  syncError = null;
  largeFilesOnDisk = [];
  lfsError = null;
  initialiseError = null;
  initialiseNeedsIdentity = false;
  ourFolderIgnoredOnDisk = true;
  accountOnDisk = null;
  signInError = null;
  publicAnswer = false;
  originOnDisk = null;
  createError = null;
  calls.created = [];
  calls.publicQuestions = [];
  calls.githubCalls = [];
  calls.openedUrls = [];
  calls.gitLogPages = [];
  calls.gitStatusReads = 0;
  calls.gitVerbs = [];
  calls.discardQuestions = [];
  comparisons.asked = [];
  comparisons.saved = [];
  savePathAnswer = '/out/diff.pdf';
  snapshotRows = [];
  snapshotFilesOnDisk = {};
  snapshotError = null;
  restores.length = 0;
  diffAnswer = { before: 'a', after: 'b' };
  diffsAsked.length = 0;
  listAnswer = [];
  cloneAnswer = '/papers/thesis';
  parentFolderAnswer = '/papers';
  clones.asked = [];
  clones.lists = 0;
  compareAnswer = async (a, b) => ({ status: 'building', older: a, newer: b, generation: 1 });
  app.activityView = 'files';
  // The stores are module-level singletons, so a test that left a sign-in waiting would make the
  // next one start from there (S10.4b).
  github.stage = 'idle';
  github.code = null;
  github.error = null;
  github.account = undefined;
  github.created = null;
  github.createError = null;
  github.creating = false;
  app.conflict = null;
  app.notice = null;
  await start();
  await openFolder('/proj');
  calls.compiles = 0; // the open triggered one; the tests care about the ones after
  calls.reads = []; // ditto for the read the open performed
});

afterEach(() => vi.useRealTimers());

describe('a project whose engine setting could not be honoured (S9.4)', () => {
  afterEach(() => {
    project.engineNotice = null;
  });

  it('says so as a notice, and still opens and builds', async () => {
    project.engineNotice = 'This project asks for pdflatex, but no latexmk with pdflatex was found on this machine.';
    await openFolder('/proj');
    expect(app.notice).toBe(project.engineNotice);
    expect(app.activePath).toBe('main.tex');
    expect(calls.compiles).toBe(1);
  });

  it('shows no notice when the setting was honoured', async () => {
    await openFolder('/proj');
    expect(app.notice).toBeNull();
  });
});

describe('an external change to the open file', () => {
  it('lands in the CRDT and rebuilds when the buffer has nothing to lose', async () => {
    disk.set('main.tex', 'hello world');
    await fileChanged('main.tex');

    expect(app.activeDoc!.text()).toBe('hello world');
    expect(app.dirty).toBe(false);
    expect(app.conflict).toBeNull();
    expect(calls.compiles).toBe(1);
  });

  it('is ignored when the disk already matches the buffer', async () => {
    await fileChanged('main.tex');
    expect(app.conflict).toBeNull();
    expect(calls.writes).toHaveLength(0);
  });

  it('asks instead of merging when the buffer is dirty, and writes nothing while it asks', async () => {
    type(' there');
    disk.set('main.tex', 'hello world');
    await fileChanged('main.tex');

    expect(app.conflict).toEqual({ path: 'main.tex', diskText: 'hello world' });
    // The buffer is untouched: no merge happened.
    expect(app.activeDoc!.text()).toBe('hello there');

    // The heart of the loop. The 700 ms debounce was armed by the keystrokes above; if it
    // fires while the bar is up it overwrites the disk and answers for the author.
    await vi.advanceTimersByTimeAsync(5000);
    expect(calls.writes).toHaveLength(0);
    expect(disk.get('main.tex')).toBe('hello world');
    expect(calls.compiles).toBe(0);
  });

  it('does not lose the buffer when the file is deleted underneath it', async () => {
    type(' there');
    disk.delete('main.tex');
    await fileChanged('main.tex');

    expect(app.activeDoc!.text()).toBe('hello there');
    expect(app.notice).toContain('no longer on disk');
    expect(app.conflict).toBeNull();
    // Unsaved edits to a file that no longer exists get written back rather than dropped.
    await vi.advanceTimersByTimeAsync(700);
    expect(disk.get('main.tex')).toBe('hello there');
  });
});

describe('multi-document tabs (S2.3)', () => {
  beforeEach(() => {
    disk.set('sections/results.tex', 'Results.');
  });

  it('opening a second file adds a tab without touching the first', async () => {
    type(' there'); // dirty main.tex, unsaved

    await openFile('sections/results.tex');

    expect(app.openTabs).toEqual(['main.tex', 'sections/results.tex']);
    expect(app.activePath).toBe('sections/results.tex');
    // The first tab's buffer is untouched — opening a second file must not save, reload, or
    // otherwise disturb what the author was in the middle of typing.
    expect(app.docs.get('main.tex')!.text()).toBe('hello there');
    expect(app.dirtyPaths.has('main.tex')).toBe(true);
    expect(calls.writes).toHaveLength(0);
  });

  it('switching back to an already-open tab does not re-read the file from disk', async () => {
    await openFile('sections/results.tex');
    calls.reads = [];

    await openFile('main.tex');
    expect(app.activePath).toBe('main.tex');
    expect(calls.reads).toEqual([]);

    await openFile('sections/results.tex');
    expect(app.activePath).toBe('sections/results.tex');
    expect(calls.reads).toEqual([]);
  });

  it('closing a tab flushes its unsaved edit, then activates a neighbour', async () => {
    await openFile('sections/results.tex');
    type(' two'); // dirty the now-active results.tex

    await closeTab('sections/results.tex');

    expect(disk.get('sections/results.tex')).toBe('Results. two');
    expect(app.openTabs).toEqual(['main.tex']);
    expect(app.activePath).toBe('main.tex');
    expect(app.dirtyPaths.has('sections/results.tex')).toBe(false);
  });

  it('compiling saves every open tab, not only the active one', async () => {
    await openFile('sections/results.tex');
    // main.tex is a background tab now; dirty it without switching back to it.
    typeInto('main.tex', ' there');
    type(' too'); // and the active tab, results.tex

    await triggerCompile();

    expect(disk.get('main.tex')).toBe('hello there');
    expect(disk.get('sections/results.tex')).toBe('Results. too');
    expect(app.dirtyPaths.size).toBe(0);
    expect(calls.compiles).toBe(1);
  });

  it('an external conflict on a background tab brings that tab to the front to ask about it', async () => {
    await openFile('sections/results.tex');
    typeInto('main.tex', ' there'); // dirty the background tab
    disk.set('main.tex', 'hello world'); // and move the disk on underneath it

    fsHandler({ path: '/proj/main.tex', exists: true });
    await vi.advanceTimersByTimeAsync(0);

    expect(app.conflict).toEqual({ path: 'main.tex', diskText: 'hello world' });
    expect(app.activePath).toBe('main.tex'); // brought forward so the bar makes sense
    expect(app.docs.get('main.tex')!.text()).toBe('hello there'); // nothing was merged
  });
});

describe('compile progress (S2.2)', () => {
  it('shows the latest engine line while a build runs, and clears it once the build ends', () => {
    compileHandler({ status: 'started', generation: 1, rootFile: 'main.tex' });
    expect(app.compile.progress).toBeNull();

    compileHandler({ status: 'progress', generation: 1, message: 'Downloading amsmath.sty' });
    expect(app.compile.progress).toBe('Downloading amsmath.sty');

    compileHandler({ status: 'progress', generation: 1, message: 'Downloading hyperref.sty' });
    expect(app.compile.progress).toBe('Downloading hyperref.sty');

    compileHandler({
      status: 'finished',
      generation: 1,
      success: true,
      pdfPath: null,
      logPath: null,
      diagnostics: [],
      durationMs: 10,
      stderr: 'Downloading amsmath.sty\nDownloading hyperref.sty\n',
    });
    expect(app.compile.progress).toBeNull();
  });

  it('ignores a progress line from a build that a newer request has already superseded', () => {
    compileHandler({ status: 'started', generation: 1, rootFile: 'main.tex' });
    compileHandler({ status: 'started', generation: 2, rootFile: 'main.tex' });

    // A line from the cancelled build 1 arriving late must not overwrite build 2's state.
    compileHandler({ status: 'progress', generation: 1, message: 'stale line' });
    expect(app.compile.progress).toBeNull();

    compileHandler({ status: 'progress', generation: 2, message: 'current line' });
    expect(app.compile.progress).toBe('current line');
  });
});

describe('shell escape by consent (S9.8)', () => {
  it('reads the folder\'s consent when the folder opens', async () => {
    expect(app.shellEscapeAllowed).toBe(false);
    shellEscapeOnDisk = true;
    await openFolder('/proj');
    expect(app.shellEscapeAllowed).toBe(true);
  });

  it('asks first, and a "not now" changes nothing and builds nothing', async () => {
    shellEscapeAnswer = false;
    await allowShellEscape();
    expect(calls.shellEscapeQuestions).toBe(1);
    expect(shellEscapeOnDisk).toBe(false);
    expect(app.shellEscapeAllowed).toBe(false);
    expect(calls.compiles).toBe(0);
  });

  it('records a yes for this folder and builds again with it', async () => {
    shellEscapeAnswer = true;
    await allowShellEscape();
    expect(shellEscapeOnDisk).toBe(true);
    expect(app.shellEscapeAllowed).toBe(true);
    expect(calls.compiles).toBe(1);

    await allowShellEscape();
    expect(calls.shellEscapeQuestions, 'already allowed: nothing to ask').toBe(1);
  });

  it('takes it back without a question', async () => {
    shellEscapeAnswer = true;
    await allowShellEscape();
    await disallowShellEscape();
    expect(calls.shellEscapeQuestions).toBe(1);
    expect(shellEscapeOnDisk).toBe(false);
    expect(app.shellEscapeAllowed).toBe(false);
  });
});

describe('the draft preview (S9.9)', () => {
  const finished = (generation: number, success: boolean): CompileEvent => ({
    status: 'finished',
    generation,
    success,
    pdfPath: success ? '/proj/.abstract-tex/build/main.pdf' : null,
    logPath: null,
    diagnostics: [],
    durationMs: 3300,
    stderr: '',
  });
  const draft = (generation: number): CompileEvent => ({
    status: 'draft',
    generation,
    chapter: 'chapters/03-method',
    pdfPath: '/proj/.abstract-tex/draft/build/main.pdf',
    durationMs: 2060,
  });

  it('asks Rust to draft the file being edited', async () => {
    await triggerCompile();
    expect(app.activePath).toBe('main.tex');
    expect(calls.compileFiles.at(-1)).toBe('main.tex');
  });

  it('shows the draft while the full build runs, then the full PDF in its place', () => {
    compileHandler({ status: 'started', generation: 1, rootFile: 'main.tex' });
    compileHandler(draft(1));
    expect(app.pdfUrl).toContain('/draft/build/main.pdf');
    expect(app.pdfDraftOf).toBe('chapters/03-method');

    compileHandler(finished(1, true));
    expect(app.pdfUrl).toContain('/build/main.pdf');
    expect(app.pdfUrl).not.toContain('/draft/');
    expect(app.pdfDraftOf).toBeNull();
  });

  it('never lets a draft replace a finished build, nor a superseded build put one on screen', () => {
    compileHandler({ status: 'started', generation: 1, rootFile: 'main.tex' });
    compileHandler(finished(1, true));
    const full = app.pdfUrl;
    compileHandler(draft(1)); // late: Rust promises this cannot happen; the screen agrees anyway
    expect(app.pdfUrl).toBe(full);

    compileHandler({ status: 'started', generation: 2, rootFile: 'main.tex' });
    compileHandler({ status: 'started', generation: 3, rootFile: 'main.tex' });
    compileHandler(draft(2));
    expect(app.pdfUrl).toBe(full);
    expect(app.pdfDraftOf).toBeNull();
  });

  it('puts the last full PDF back when the build the draft stood in for fails', () => {
    compileHandler({ status: 'started', generation: 1, rootFile: 'main.tex' });
    compileHandler(finished(1, true));
    const full = app.pdfUrl;

    compileHandler({ status: 'started', generation: 2, rootFile: 'main.tex' });
    compileHandler(draft(2));
    compileHandler(finished(2, false));
    expect(app.pdfUrl).toBe(full);
    expect(app.pdfDraftOf).toBeNull();
  });

  it('searches the SyncTeX of whichever PDF is on screen', async () => {
    compileHandler({ status: 'started', generation: 1, rootFile: 'main.tex' });
    await syncTexForward('main.tex', 3);
    compileHandler(draft(1));
    await syncTexForward('main.tex', 3);
    await syncTexInverse(1, 10, 20);
    compileHandler(finished(1, true));
    await syncTexInverse(1, 10, 20);
    expect(calls.synctexDraft).toEqual([false, true, true, false]);
  });
});

describe('resolving a conflict', () => {
  beforeEach(async () => {
    type(' there');
    disk.set('main.tex', 'hello world');
    await fileChanged('main.tex');
    expect(app.conflict).not.toBeNull();
  });

  it('"keep mine" overwrites the disk with the buffer', async () => {
    await resolveConflict('keep-mine');
    expect(app.conflict).toBeNull();
    expect(disk.get('main.tex')).toBe('hello there');
    expect(app.activeDoc!.text()).toBe('hello there');
    expect(app.dirty).toBe(false);
  });

  it('"load from disk" takes the disk version into the buffer and rebuilds', async () => {
    await resolveConflict('load-disk');
    expect(app.conflict).toBeNull();
    expect(app.activeDoc!.text()).toBe('hello world');
    expect(app.dirty).toBe(false);
    expect(calls.writes).toHaveLength(0);
    expect(calls.compiles).toBe(1);
  });

  it('resumes normal saving once answered', async () => {
    await resolveConflict('load-disk');
    type('!');
    await vi.advanceTimersByTimeAsync(700);
    expect(disk.get('main.tex')).toBe('hello world!');
  });
});

describe('quick open (S2.4)', () => {
  it('toggles, and picking a file opens it and puts the list away', async () => {
    disk.set('sections/results.tex', 'Results.');
    toggleQuickOpen();
    expect(app.quickOpenVisible).toBe(true);
    toggleQuickOpen();
    expect(app.quickOpenVisible).toBe(false);

    toggleQuickOpen();
    await quickOpenPick('sections/results.tex');
    expect(app.quickOpenVisible).toBe(false);
    expect(app.activePath).toBe('sections/results.tex');
    expect(app.openTabs).toEqual(['main.tex', 'sections/results.tex']);
  });

  it('does nothing without a project open', () => {
    app.project = null; // a list of the files in no folder would be an empty box
    toggleQuickOpen();
    expect(app.quickOpenVisible).toBe(false);
  });
});

describe('diagnostics (S2.7)', () => {
  const underscore = {
    title: '_ used outside maths',
    explanation: '`_` means "subscript" and only works inside maths mode.',
    line: 87,
    file: 'main.tex',
    severity: 'error' as const,
    rule: 'missing-dollar',
    rawMessage: 'Missing $ inserted.',
    fix: { description: 'Escape as \\_', find: '_', replace: '\\_' },
  };
  const citation = {
    title: '`knuth1984` is cited but not in the bibliography',
    explanation: 'No entry with the key `knuth1984` was found.',
    line: 7,
    file: 'main.tex',
    severity: 'warning' as const,
    rule: 'undefined-citation',
    rawMessage: "Citation `knuth1984' on page 1 undefined on input line 7.",
    fix: null,
  };

  function finished(success: boolean, diagnostics: Array<typeof underscore | typeof citation>) {
    compileHandler({ status: 'started', generation: 1, rootFile: 'main.tex' });
    compileHandler({
      status: 'finished',
      generation: 1,
      success,
      pdfPath: success ? '/proj/.abstract-tex/build/main.pdf' : null,
      logPath: '/proj/.abstract-tex/build/main.log',
      diagnostics,
      durationMs: 10,
      stderr: '',
    });
  }

  it('a failed build opens the drawer with the sentence, never the raw log', () => {
    finished(false, [underscore]);
    expect(app.drawerOpen).toBe(true);
    expect(app.showRawLog).toBe(false);
    expect(app.errorCount).toBe(1);
    expect(app.warningCount).toBe(0);
    expect(app.compile.diagnostics[0]!.title).toBe('_ used outside maths');
  });

  it('a clean build with warnings stays quiet but counts them', () => {
    finished(true, [citation]);
    expect(app.drawerOpen).toBe(false); // never shouting when the PDF was produced
    expect(app.warningCount).toBe(1);
    expect(app.errorCount).toBe(0);
  });

  it('jumping to a diagnostic brings its own file to the front first', async () => {
    disk.set('notes.tex', 'notes');
    await openFile('notes.tex');
    expect(app.activePath).toBe('notes.tex');

    await jumpToDiagnostic(underscore);
    expect(app.activePath).toBe('main.tex');
    expect(app.jumpRequest?.line).toBe(87);
  });

  it('jumping to a diagnostic in a different file opens that file, not the root (S5.6 wiring)', async () => {
    disk.set('chapters/intro.tex', 'text\n');
    await jumpToDiagnostic({ ...underscore, file: 'chapters/intro.tex', line: 1 });
    expect(app.activePath).toBe('chapters/intro.tex');
    expect(app.jumpRequest?.line).toBe(1);
  });

  it('a diagnostic with no file at all falls back to the root file', async () => {
    disk.set('notes.tex', 'notes');
    await openFile('notes.tex');
    await jumpToDiagnostic({ ...underscore, file: null });
    expect(app.activePath).toBe('main.tex');
  });

  it('a diagnostic with no line goes nowhere', async () => {
    const before = app.jumpRequest;
    await jumpToDiagnostic({ ...underscore, line: null });
    expect(app.jumpRequest).toBe(before);
  });
});

describe('applying a diagnostic fix (S6.2)', () => {
  const withAmpersand = {
    title: '`&` used outside a table',
    explanation: 'If you meant a literal ampersand, write `\\&` instead.',
    line: 3,
    file: 'main.tex',
    severity: 'error' as const,
    rule: 'misplaced-alignment-tab',
    rawMessage: 'Misplaced alignment tab character &.',
    fix: { description: 'Escape as \\&', find: '&', replace: '\\&' },
  };

  /** `main.tex` is already open by the outer `beforeEach` (`openFolder` opens the root file),
   * so replacing `disk`'s copy would not be seen — a fix always acts on the live buffer, the
   * same as any other edit. Setting the already-open document's own text is what a real external
   * change to that content would look like. */
  function setMainTexBuffer(text: string): void {
    const doc = app.docs.get('main.tex')!;
    doc.ytext.delete(0, doc.ytext.length);
    doc.ytext.insert(0, text);
  }

  beforeEach(() => {
    setMainTexBuffer('one\ntwo\nSalt & pepper.\n');
  });

  it('finds the file, edits it through the CRDT, and saves it on the usual debounce', async () => {
    const applied = await applyDiagnosticFix(withAmpersand);
    expect(applied).toBe(true);
    expect(app.activePath).toBe('main.tex');
    expect(app.jumpRequest?.line).toBe(3);

    await vi.advanceTimersByTimeAsync(700);
    expect(disk.get('main.tex')).toBe('one\ntwo\nSalt \\& pepper.\n');
  });

  it('opens the file first when the fix belongs to a tab that is not open yet', async () => {
    disk.set('chapters/intro.tex', 'Salt & pepper.\n');
    const applied = await applyDiagnosticFix({ ...withAmpersand, file: 'chapters/intro.tex', line: 1 });
    expect(applied).toBe(true);
    expect(app.activePath).toBe('chapters/intro.tex');
    await vi.advanceTimersByTimeAsync(700);
    expect(disk.get('chapters/intro.tex')).toBe('Salt \\& pepper.\n');
  });

  it('declines, and leaves a notice, when the named line does not hold the fix anymore', async () => {
    setMainTexBuffer('one\ntwo\nnothing to escape here\n');
    const applied = await applyDiagnosticFix(withAmpersand);
    expect(applied).toBe(false);
    expect(app.notice).toContain('main.tex');
  });

  it('does nothing for a diagnostic with no fix', async () => {
    const applied = await applyDiagnosticFix({ ...withAmpersand, fix: null });
    expect(applied).toBe(false);
  });

  it('does nothing for a diagnostic with no file and no project root to fall back to', async () => {
    app.project = null;
    const applied = await applyDiagnosticFix({ ...withAmpersand, file: null });
    expect(applied).toBe(false);
  });
});

describe('applying a bibliography finding fix (S8.8)', () => {
  const linkedNotNamed: Finding = {
    rule: 'linked-not-named',
    severity: 'error',
    message: "'zotero/Thesis.bib' is linked but the document does not name it.",
    jump: { kind: 'texLine', file: 'main.tex', line: 2 },
    fix: {
      description: 'Add zotero/Thesis to \\bibliography',
      find: '\\bibliography{references}',
      replace: '\\bibliography{references,zotero/Thesis}',
    },
  };

  function setMainTexBuffer(text: string): void {
    const doc = app.docs.get('main.tex')!;
    doc.ytext.delete(0, doc.ytext.length);
    doc.ytext.insert(0, text);
  }

  it('edits the named line through the CRDT, the same way a diagnostic fix does', async () => {
    setMainTexBuffer('\\cite{zot2020}\n\\bibliography{references}\n');
    const applied = await applyFindingFix(linkedNotNamed);
    expect(applied).toBe(true);
    expect(app.jumpRequest?.line).toBe(2);
    await vi.advanceTimersByTimeAsync(700);
    expect(disk.get('main.tex')).toBe('\\cite{zot2020}\n\\bibliography{references,zotero/Thesis}\n');
  });

  it('declines with a notice when the line changed since the index was built', async () => {
    setMainTexBuffer('\\cite{zot2020}\n\\bibliography{other}\n');
    expect(await applyFindingFix(linkedNotNamed)).toBe(false);
    expect(app.notice).toContain('main.tex');
  });

  it('does nothing for a finding with no fix, or one that does not point at a .tex line', async () => {
    expect(await applyFindingFix({ ...linkedNotNamed, fix: null })).toBe(false);
    expect(await applyFindingFix({ ...linkedNotNamed, jump: { kind: 'missingFile', file: 'zotero/Thesis.bib' } })).toBe(
      false,
    );
  });
});

describe('the drawer v1 (S6.3)', () => {
  const underscore = {
    title: '_ used outside maths',
    explanation: '`_` means "subscript" and only works inside maths mode.',
    line: 59,
    file: 'sections/background.tex',
    severity: 'error' as const,
    rule: 'missing-dollar',
    rawMessage: 'Missing $ inserted.',
    fix: { description: 'Escape as \\_', find: '_', replace: '\\_' },
  };

  function finished(generation: number, success: boolean, diagnostics: Array<typeof underscore>) {
    compileHandler({ status: 'started', generation, rootFile: 'main.tex' });
    compileHandler({
      status: 'finished',
      generation,
      success,
      pdfPath: success ? '/proj/.abstract-tex/build/main.pdf' : null,
      logPath: '/proj/.abstract-tex/build/main.log',
      diagnostics,
      durationMs: 10,
      stderr: '',
    });
  }

  beforeEach(() => {
    app.drawerOpen = false;
    app.showRawLog = false;
    app.rawLogFocus = null;
    setDrawerFilter({ severity: 'all', activeFileOnly: false });
  });

  it("a card's \"Raw log\" opens the drawer on the raw view, pointed at that diagnostic's own words", async () => {
    logOnDisk = 'This is XeTeX\n! Missing $ inserted.\nl.59 A stray underscore_\n';
    finished(1, false, [underscore]);
    app.drawerOpen = false; // the author closed it; the button must reopen it

    await showRawLogFor(underscore);
    expect(app.drawerOpen).toBe(true);
    expect(app.showRawLog).toBe(true);
    expect(app.rawLog).toBe(logOnDisk);
    expect(app.rawLogFocus?.rawMessage).toBe('Missing $ inserted.');
  });

  it('asking for the same card twice is two distinct requests, so the view scrolls back to it', async () => {
    await showRawLogFor(underscore);
    const first = app.rawLogFocus!.nonce;
    await showRawLogFor(underscore);
    expect(app.rawLogFocus!.nonce).toBeGreaterThan(first);
  });

  it('re-reads the log when a build finishes while the raw view is open (regression: it showed the old log)', async () => {
    logOnDisk = 'first build';
    finished(1, false, [underscore]);
    await toggleRawLog();
    expect(app.rawLog).toBe('first build');

    logOnDisk = 'second build';
    finished(2, false, [underscore]);
    await vi.advanceTimersByTimeAsync(0);
    expect(app.showRawLog).toBe(true);
    expect(app.rawLog).toBe('second build');
  });

  it('a finished build drops the highlight request, whose words belonged to the log it replaced', async () => {
    await showRawLogFor(underscore);
    expect(app.rawLogFocus).not.toBeNull();
    finished(2, false, [underscore]);
    expect(app.rawLogFocus).toBeNull();
  });

  it('a clean build leaves the raw view and does not re-read a log nobody is looking at', async () => {
    finished(1, false, [underscore]);
    await toggleRawLog();
    expect(calls.logReads).toBe(1);
    finished(2, true, []);
    await vi.advanceTimersByTimeAsync(0);
    expect(app.showRawLog).toBe(false);
    expect(app.drawerOpen).toBe(false);
    expect(calls.logReads).toBe(1);
  });

  it('a failed build with the raw view closed does not read the log either — it is never the default', async () => {
    finished(1, false, [underscore]);
    await vi.advanceTimersByTimeAsync(0);
    expect(app.drawerOpen).toBe(true);
    expect(app.showRawLog).toBe(false);
    expect(calls.logReads).toBe(0);
  });

  it('opening another project forgets the previous one\'s raw log', async () => {
    logOnDisk = 'old project';
    await toggleRawLog();
    expect(app.rawLog).toBe('old project');
    await openFolder('/other');
    expect(app.showRawLog).toBe(false);
    expect(app.rawLog).toBe('');
    expect(app.rawLogFocus).toBeNull();
  });

  it('setDrawerFilter changes one part and keeps the rest', () => {
    setDrawerFilter({ severity: 'errors' });
    expect(app.drawerFilter).toEqual({ severity: 'errors', activeFileOnly: false });
    setDrawerFilter({ activeFileOnly: true });
    expect(app.drawerFilter).toEqual({ severity: 'errors', activeFileOnly: true });
  });

  it('the filter survives a new build — an author chasing errors is still chasing them', () => {
    setDrawerFilter({ severity: 'errors' });
    finished(1, false, [underscore]);
    expect(app.drawerFilter.severity).toBe('errors');
  });

  it('toggleDrawer flips the drawer, and both it and the raw log are palette commands', () => {
    expect(app.drawerOpen).toBe(false);
    toggleDrawer();
    expect(app.drawerOpen).toBe(true);
    toggleDrawer();
    expect(app.drawerOpen).toBe(false);

    const ids = allCommands().map((c) => c.id);
    expect(ids).toContain('toggle-drawer');
    expect(ids).toContain('show-raw-log');
  });

  it('the "Show raw log" command reaches the log with the drawer closed, in one action', async () => {
    logOnDisk = 'the log';
    allCommands().find((c) => c.id === 'show-raw-log')!.run();
    await vi.advanceTimersByTimeAsync(0);
    expect(app.drawerOpen).toBe(true);
    expect(app.showRawLog).toBe(true);
    expect(app.rawLog).toBe('the log');
  });
});

describe('the language server (S3.2)', () => {
  /** Only the document-sync messages, in order, for readable assertions. */
  const methods = () => calls.lsp.map((c) => c.method);

  it('starts when a project opens and tells the server about the first file', async () => {
    expect(calls.lspStarts).toBe(1);
    expect(app.lspReady).toBe(true);
    expect(app.lspMessage).toBeNull();

    const opens = calls.lsp.filter((c) => c.method === 'textDocument/didOpen');
    expect(opens).toHaveLength(1);
    expect(opens[0]?.params.textDocument).toMatchObject({
      uri: 'file:///proj/main.tex',
      languageId: 'latex',
      version: 1,
      text: 'hello',
    });
  });

  it('opening another tab opens it on the server too', async () => {
    disk.set('sections/results.tex', 'Results.');
    calls.lsp = [];
    await openFile('sections/results.tex');

    const opens = calls.lsp.filter((c) => c.method === 'textDocument/didOpen');
    expect(opens[0]?.params.textDocument.uri).toBe('file:///proj/sections/results.tex');
  });

  it('a save sends the new text and then didSave, with a higher version', async () => {
    calls.lsp = [];
    type(' there');
    await vi.advanceTimersByTimeAsync(700); // the debounced save

    expect(methods()).toEqual(['textDocument/didChange', 'textDocument/didSave']);
    const change = calls.lsp[0]!;
    expect(change.params.textDocument.version).toBe(2);
    expect(change.params.contentChanges[0].text).toBe('hello there');
  });

  it('closing a tab closes it on the server', async () => {
    disk.set('notes.tex', 'notes');
    await openFile('notes.tex');
    calls.lsp = [];
    await closeTab('notes.tex');

    const closes = calls.lsp.filter((c) => c.method === 'textDocument/didClose');
    expect(closes[0]?.params.textDocument.uri).toBe('file:///proj/notes.tex');
  });

  /** The reason the restart path exists: a fresh TexLab has never heard of these files, and
   * carrying on from the old version numbers would leave the two permanently out of step. */
  it('re-opens every document at version 1 after the server restarts', async () => {
    disk.set('notes.tex', 'notes');
    await openFile('notes.tex');
    type(' more'); // bump main.tex past version 1
    await vi.advanceTimersByTimeAsync(700);
    calls.lsp = [];

    lspHandler({ kind: 'restarted', restarts: 1 });
    await vi.advanceTimersByTimeAsync(0);

    const reopened = calls.lsp.filter((c) => c.method === 'textDocument/didOpen');
    expect(reopened.map((c) => c.params.textDocument.uri)).toEqual([
      'file:///proj/main.tex',
      'file:///proj/notes.tex',
    ]);
    for (const open of reopened) {
      expect(open.params.textDocument.version).toBe(1);
    }
    // And the latest text, not what the file was first opened with. `notes.tex` was the active
    // tab when ' more' was typed, so it is the one that changed.
    expect(reopened[1]?.params.textDocument.text).toBe('notes more');
    expect(reopened[0]?.params.textDocument.text).toBe('hello');
    expect(app.lspReady).toBe(true);
  });

  it('a stopped server is reported quietly and stops further traffic', async () => {
    lspHandler({ kind: 'stopped', message: 'TexLab crashed 5 times' });
    calls.lsp = [];

    expect(app.lspReady).toBe(false);
    expect(app.lspMessage).toBe('TexLab crashed 5 times');
    // A notice is for something the author must act on; losing completion is not that.
    expect(app.notice).toBeNull();

    type(' more');
    await vi.advanceTimersByTimeAsync(700);
    expect(calls.lsp).toHaveLength(0);
  });

  /** The commitment that matters most here: DESIGN.md §2 number 6. No TexLab, no language
   * features — and everything else works exactly as before. */
  it('opens the project and compiles normally when TexLab is missing', async () => {
    lspStartError = 'No TexLab binary found. Run `pnpm fetch-lsp`.';
    calls.lsp = [];
    await openFolder('/proj');

    expect(app.lspReady).toBe(false);
    expect(app.lspMessage).toContain('No TexLab binary found');
    expect(app.notice).toBeNull(); // not raised at the author
    expect(calls.lsp).toHaveLength(0); // nothing sent to a server that is not there

    // The editor is fully alive: the file opened, and typing still saves and rebuilds.
    expect(app.activePath).toBe('main.tex');
    calls.compiles = 0;
    type(' there');
    await vi.advanceTimersByTimeAsync(700);
    expect(disk.get('main.tex')).toBe('hello there');
    expect(calls.compiles).toBe(1);
  });
});

describe('hover, go-to-definition, document symbols (S3.3c)', () => {
  it('lspHover asks the server and returns its answer', async () => {
    lspRequestAnswer = { contents: 'undefined command' };
    const hover = await lspHover('main.tex', 0, 1);
    expect(hover).toEqual({ contents: 'undefined command' });
  });

  it('lspHover returns null with no project open', async () => {
    // `openFolder`/`start` in `beforeEach` already opened /proj; simulate "no project" the way
    // the rest of this file does not need to for its other tests, by clearing it directly.
    app.project = null;
    expect(await lspHover('main.tex', 0, 0)).toBeNull();
  });

  it('lspHover returns null when the server is not ready', async () => {
    lspHandler({ kind: 'stopped', message: 'gone' });
    expect(await lspHover('main.tex', 0, 0)).toBeNull();
  });

  it('lspGoToDefinition moves the cursor within the same file', async () => {
    lspRequestAnswer = {
      uri: 'file:///proj/main.tex',
      range: { start: { line: 3, character: 0 }, end: { line: 3, character: 5 } },
    };
    const found = await lspGoToDefinition('main.tex', 0, 0);

    expect(found).toBe(true);
    expect(app.activePath).toBe('main.tex');
    expect(app.jumpRequest?.line).toBe(4); // LSP's 0-based line 3 -> CodeMirror's 1-based line 4
  });

  it('lspGoToDefinition opens the target file when the definition is elsewhere', async () => {
    disk.set('sections/results.tex', 'Results.\nSee \\label{fig:one}\n');
    lspRequestAnswer = {
      uri: 'file:///proj/sections/results.tex',
      range: { start: { line: 1, character: 4 }, end: { line: 1, character: 18 } },
    };
    const found = await lspGoToDefinition('main.tex', 0, 0);

    expect(found).toBe(true);
    expect(app.activePath).toBe('sections/results.tex');
    expect(app.jumpRequest?.line).toBe(2);
  });

  it('lspGoToDefinition takes the first of several locations', async () => {
    lspRequestAnswer = [
      { uri: 'file:///proj/main.tex', range: { start: { line: 0, character: 0 }, end: { line: 0, character: 1 } } },
      { uri: 'file:///proj/main.tex', range: { start: { line: 9, character: 0 }, end: { line: 9, character: 1 } } },
    ];
    await lspGoToDefinition('main.tex', 0, 0);
    expect(app.jumpRequest?.line).toBe(1);
  });

  it('lspGoToDefinition returns false and does nothing for a null answer', async () => {
    lspRequestAnswer = null;
    const before = app.activePath;
    expect(await lspGoToDefinition('main.tex', 0, 0)).toBe(false);
    expect(app.activePath).toBe(before);
  });

  it('lspGoToDefinition returns false when the server is not ready', async () => {
    lspHandler({ kind: 'stopped', message: 'gone' });
    lspRequestAnswer = {
      uri: 'file:///proj/main.tex',
      range: { start: { line: 0, character: 0 }, end: { line: 0, character: 1 } },
    };
    expect(await lspGoToDefinition('main.tex', 0, 0)).toBe(false);
  });

  it('lspDocumentSymbols flattens the server\'s tree', async () => {
    lspRequestAnswer = [
      {
        name: 'Introduction',
        kind: 1,
        range: { start: { line: 0, character: 0 }, end: { line: 0, character: 0 } },
        selectionRange: { start: { line: 0, character: 0 }, end: { line: 0, character: 0 } },
      },
    ];
    const symbols = await lspDocumentSymbols('main.tex');
    expect(symbols).toEqual([{ name: 'Introduction', detail: null, kind: 1, line: 1, depth: 0 }]);
  });

  it('lspDocumentSymbols returns an empty list when the server is not ready', async () => {
    lspHandler({ kind: 'stopped', message: 'gone' });
    expect(await lspDocumentSymbols('main.tex')).toEqual([]);
  });
});

describe('publishDiagnostics reaching the gutter (S3.3b)', () => {
  /** One wire diagnostic, 0-based as the server sends it. */
  function wire(line: number, severity: 1 | 2 | 3 | 4 | undefined, message = 'Undefined reference.') {
    return {
      range: { start: { line, character: 0 }, end: { line, character: 6 } },
      severity,
      message,
      source: 'texlab',
    };
  }

  /** Push a notification through the same path a real event takes. */
  function publish(uri: string, diagnostics: unknown[]): void {
    lspHandler({ kind: 'notification', method: 'textDocument/publishDiagnostics', params: { uri, diagnostics } });
  }

  it('puts a diagnostic on the right 1-based line of the file it names', () => {
    publish('file:///proj/main.tex', [wire(3, 1)]);

    const rows = lspDiagnosticsFor('main.tex');
    expect(rows).toHaveLength(1);
    expect(rows[0]).toMatchObject({ startLine: 4, severity: 'error', title: 'Undefined reference.' });
    expect(app.lspDiagnosticsVersion).toBeGreaterThan(0);
  });

  /** The case that rots silently: the server saying "this file is clean now". */
  it('a second publish with an empty array leaves the file with nothing', () => {
    publish('file:///proj/main.tex', [wire(3, 1)]);
    expect(lspDiagnosticsFor('main.tex')).toHaveLength(1);
    const versionAfterFirst = app.lspDiagnosticsVersion;

    publish('file:///proj/main.tex', []);
    expect(lspDiagnosticsFor('main.tex')).toEqual([]);
    // The gutter only redraws if the counter moved, so an empty publish must still bump it.
    expect(app.lspDiagnosticsVersion).toBeGreaterThan(versionAfterFirst);
  });

  it('a publish for a file with no open tab is kept and changes nothing about the active one', () => {
    publish('file:///proj/main.tex', [wire(1, 2)]);
    publish('file:///proj/sections/results.tex', [wire(9, 1)]);

    expect(app.openTabs).toEqual(['main.tex']);
    expect(lspDiagnosticsFor('sections/results.tex').map((r) => r.startLine)).toEqual([10]);
    expect(lspDiagnosticsFor('main.tex').map((r) => r.startLine)).toEqual([2]);
  });

  it('drops a malformed payload without throwing', () => {
    publish('file:///proj/main.tex', [wire(3, 1)]);
    const before = app.lspDiagnosticsVersion;

    expect(() => {
      lspHandler({ kind: 'notification', method: 'textDocument/publishDiagnostics', params: { diagnostics: [] } });
      lspHandler({
        kind: 'notification',
        method: 'textDocument/publishDiagnostics',
        params: { uri: 'file:///proj/main.tex', diagnostics: 'not an array' },
      });
      lspHandler({ kind: 'notification', method: 'textDocument/publishDiagnostics', params: null });
    }).not.toThrow();

    // Nothing was stored and nothing was withdrawn: the good publish still stands.
    expect(lspDiagnosticsFor('main.tex')).toHaveLength(1);
    expect(app.lspDiagnosticsVersion).toBe(before);
  });

  it('ignores a notification for any other method', () => {
    publish('file:///proj/main.tex', [wire(3, 1)]);
    const before = app.lspDiagnosticsVersion;

    lspHandler({ kind: 'notification', method: '$/progress', params: { token: 1, value: {} } });
    expect(app.lspDiagnosticsVersion).toBe(before);
    expect(lspDiagnosticsFor('main.tex')).toHaveLength(1);
  });

  it('a stopped server takes its diagnostics with it', () => {
    publish('file:///proj/main.tex', [wire(3, 1)]);
    lspHandler({ kind: 'stopped', message: 'TexLab crashed 5 times' });

    expect(lspDiagnosticsFor('main.tex')).toEqual([]);
  });

  it('a restart empties the store before the resync republishes', async () => {
    publish('file:///proj/main.tex', [wire(3, 1)]);
    lspHandler({ kind: 'restarted', restarts: 1 });
    await vi.advanceTimersByTimeAsync(0);

    expect(lspDiagnosticsFor('main.tex')).toEqual([]);
  });

  /** The design decision this loop records, asserted rather than assumed: LSP diagnostics reach
   * the gutter and nothing else. Two cadences in one counter makes the status bar flicker per
   * keystroke, and the drawer's contract is an explained sentence, not a raw server string. */
  it('never touches the drawer, the compile diagnostics, or the two counts', () => {
    app.compile = { ...app.compile, diagnostics: [] };
    const drawerWasOpen = app.drawerOpen;

    publish('file:///proj/main.tex', [wire(3, 1), wire(8, 2)]);

    expect(app.compile.diagnostics).toEqual([]);
    expect(app.errorCount).toBe(0);
    expect(app.warningCount).toBe(0);
    expect(app.drawerOpen).toBe(drawerWasOpen);
  });

  it('answers with nothing for a path when no project is open', () => {
    app.project = null;
    expect(lspDiagnosticsFor('main.tex')).toEqual([]);
  });
});

describe('syncTexForward (S3.4)', () => {
  it('turns a hit into a scroll request the PDF pane can react to', async () => {
    synctexForwardAnswer = { page: 2, x: 100, y: 200 };
    await syncTexForward('main.tex', 7);
    expect(app.syncTexScrollRequest).toMatchObject({ page: 2, x: 100, y: 200 });
  });

  it('bumps the nonce on every call so a repeat request to the same spot still fires', async () => {
    await syncTexForward('main.tex', 7);
    const first = app.syncTexScrollRequest!.nonce;
    await syncTexForward('main.tex', 7);
    expect(app.syncTexScrollRequest!.nonce).not.toBe(first);
  });

  it('is silent when there is nothing typeset for the line yet — not a notice', async () => {
    synctexForwardAnswer = new Error('Nothing typeset for main.tex:7 in the last build.');
    app.syncTexScrollRequest = null;
    app.notice = null;
    await syncTexForward('main.tex', 7);
    expect(app.syncTexScrollRequest).toBeNull();
    expect(app.notice).toBeNull();
  });

  it('does nothing with no project open', async () => {
    app.project = null;
    await syncTexForward('main.tex', 7);
    expect(app.syncTexScrollRequest).toBeNull();
  });
});

describe('syncTexInverse (S3.5)', () => {
  it('moves the cursor in the already-open tab the click resolved to', async () => {
    synctexInverseAnswer = { file: 'main.tex', line: 5 };
    await syncTexInverse(1, 10, 20);
    expect(app.activePath).toBe('main.tex');
    expect(app.jumpRequest?.line).toBe(5);
  });

  it('opens the file first when the click resolves to a tab that is not open yet', async () => {
    disk.set('sections/intro.tex', 'Intro');
    synctexInverseAnswer = { file: 'sections/intro.tex', line: 1 };
    await syncTexInverse(1, 10, 20);
    expect(app.activePath).toBe('sections/intro.tex');
  });

  it('does nothing when the click resolves outside the project', async () => {
    synctexInverseAnswer = { file: null, line: 0 };
    const before = app.activePath;
    await syncTexInverse(1, 10, 20);
    expect(app.activePath).toBe(before);
  });

  it('reports a notice when the backend has nothing on that page', async () => {
    synctexInverseAnswer = new Error('Nothing on page 9 of the last build near that point.');
    await syncTexInverse(9, 0, 0);
    expect(app.notice).toContain('Nothing on page 9');
  });
});

describe('the Document map (S4.2)', () => {
  it('scans the active buffer', async () => {
    type('\\section{Intro}'); // appended after the beforeEach's 'hello', all on line 1
    await refreshOutline();
    expect(app.outline).toEqual([{ kind: 'section', title: 'Intro', line: 1, level: 2 }]);
  });

  it('refills for the newly active tab on switch, including a tab that was already open', async () => {
    // `openFile` fires its outline refresh without awaiting it — the same fire-and-forget shape
    // as `tellServer` — so a settled promise only means the *tab switch* is done; a further
    // microtask flush is what "and then it merged the server's answer in" needs in a test.
    disk.set('sections/results.tex', '\\section{Results}\n');
    await openFile('sections/results.tex');
    await vi.advanceTimersByTimeAsync(0);
    expect(app.outline).toEqual([{ kind: 'section', title: 'Results', line: 1, level: 2 }]);

    await openFile('main.tex');
    await vi.advanceTimersByTimeAsync(0);
    expect(app.outline).toEqual([]); // main.tex is still just 'hello' — no sectioning commands

    await openFile('sections/results.tex'); // switching back, not re-reading from disk
    await vi.advanceTimersByTimeAsync(0);
    expect(app.outline).toEqual([{ kind: 'section', title: 'Results', line: 1, level: 2 }]);
  });

  it("merges the server's document symbols in once it is ready, preferring them over the scan", async () => {
    type('\\section{Local}'); // the scan would find this on its own
    lspRequestAnswer = [
      {
        name: 'Server Intro',
        kind: 1,
        range: { start: { line: 0, character: 0 }, end: { line: 0, character: 0 } },
        selectionRange: { start: { line: 0, character: 0 }, end: { line: 0, character: 0 } },
      },
    ];
    await refreshOutline();
    // The server's answer describes line 1 too (0-based 0 -> 1-based 1), so it wins there —
    // the scanned 'Local' section is dropped rather than shown twice.
    expect(app.outline).toEqual([{ kind: 'section', title: 'Server Intro', line: 1, level: 0 }]);
  });

  it('scans without the server when it is not ready', async () => {
    lspHandler({ kind: 'stopped', message: 'gone' });
    type('\\section{Offline}');
    await refreshOutline();
    expect(app.outline).toEqual([{ kind: 'section', title: 'Offline', line: 1, level: 2 }]);
  });

  it('clears when the project closes', async () => {
    type('\\section{Intro}');
    await refreshOutline();
    expect(app.outline.length).toBe(1);

    await openFolder('/proj'); // closeAllDocuments runs before the new project is opened
    expect(app.outline).toEqual([]);
  });

  it("goToOutlineItem moves the cursor to the item's line", () => {
    goToOutlineItem({ kind: 'section', title: 'X', line: 42, level: 0 });
    expect(app.jumpRequest?.line).toBe(42);
  });

  it('debounces a buffer change into one rescan ~250 ms after typing stops', async () => {
    await refreshOutline();
    expect(app.outline).toEqual([]); // 'hello' has none of the five kinds

    type('\\section{New}');
    // Nothing yet: onTextChange must not reach the scan on the keystroke path itself
    // (DESIGN.md §2's <16 ms budget is the reason this is debounced at all).
    expect(app.outline).toEqual([]);

    await vi.advanceTimersByTimeAsync(249);
    expect(app.outline).toEqual([]);

    await vi.advanceTimersByTimeAsync(1);
    expect(app.outline).toEqual([{ kind: 'section', title: 'New', line: 1, level: 2 }]);
  });
});

describe('the bibliography index (S7.2)', () => {
  const smith = {
    key: 'smith2019',
    entryType: 'article',
    author: 'Smith, Jane',
    year: '2019',
    title: 'A Title',
    file: 'refs.bib',
    span: { start: 0, end: 80 },
  };

  it('is fetched once when a folder opens, and cleared first', async () => {
    bibliographyOnDisk = {
      files: [{ path: 'refs.bib', exists: true, entryCount: 1, problems: [], origin: { kind: 'named', file: 'main.tex', line: 9 } }],
      entries: [smith],
      citations: [{ key: 'smith2019', file: 'main.tex', line: 3 }],
      hasNociteStar: false,
    };
    await openFolder('/proj');
    expect(bibliography.entries.get('smith2019')).toEqual(smith);
    expect(bibliography.citations.get('smith2019')).toEqual([{ key: 'smith2019', file: 'main.tex', line: 3 }]);
    expect(bibliography.missingFiles).toEqual([]);
  });

  it('replaces its snapshot whole when Rust says the index changed', async () => {
    expect(bibliography.entries.size).toBe(0);
    bibliographyHandler({
      files: [
        { path: 'refs.bib', exists: true, entryCount: 1, problems: [], origin: { kind: 'named', file: 'main.tex', line: 9 } },
        { path: 'missing.bib', exists: false, entryCount: 0, problems: [], origin: { kind: 'linked' } },
      ],
      entries: [smith],
      citations: [],
      hasNociteStar: false,
    });
    expect(bibliography.entries.get('smith2019')).toEqual(smith);
    expect(bibliography.missingFiles).toEqual(['missing.bib']);
  });
});

describe('the Source Control view (S10.3a)', () => {
  const modified = { path: 'sections/intro.tex', kind: 'modified' as const, renamedFrom: null };
  const untracked = { path: 'notes.tex', kind: 'untracked' as const, renamedFrom: null };

  it('asks Rust what changed when a folder opens, and never polls after that', async () => {
    gitOnDisk = { staged: [], unstaged: [modified], conflicted: [] };
    await openFolder('/proj');
    await vi.advanceTimersByTimeAsync(0);
    expect(git.isRepository).toBe(true);
    expect(git.unstagedRows.map((r) => `${r.name} ${r.dir} ${r.letter}`)).toEqual(['intro.tex sections M']);

    const readsSoFar = calls.gitStatusReads;
    await vi.advanceTimersByTimeAsync(10_000);
    expect(calls.gitStatusReads).toBe(readsSoFar);
  });

  it('a folder that is not in Git is a state to describe, not an error', async () => {
    gitOnDisk = null;
    await openFolder('/proj');
    await vi.advanceTimersByTimeAsync(0);
    expect(git.isRepository).toBe(false);
    expect(git.error).toBeNull();
    // And nothing about it interrupts the author: no notice, and the editor opened as usual.
    expect(app.notice).toBeNull();
    expect(app.activePath).toBe('main.tex');
  });

  it('coalesces a burst of "something changed" into one read', async () => {
    calls.gitStatusReads = 0;
    for (let i = 0; i < 6; i++) gitStatusHandler();
    await vi.advanceTimersByTimeAsync(200);
    expect(calls.gitStatusReads).toBe(1);
  });

  it('refreshes after a save, which the watcher itself never reports', async () => {
    gitOnDisk = { staged: [], unstaged: [], conflicted: [] };
    calls.gitStatusReads = 0;
    type('\\section{New}');
    // The 700 ms save debounce writes the file; Rust emits `git:status-changed` for our own
    // write because the echo filter means the watcher will not (commands.rs).
    await vi.advanceTimersByTimeAsync(800);
    gitStatusHandler();
    await vi.advanceTimersByTimeAsync(200);
    expect(calls.gitStatusReads).toBe(1);
  });

  it('stages a path and learns the new lists from the event, not from the caller', async () => {
    gitOnDisk = { staged: [], unstaged: [modified], conflicted: [] };
    await refreshGitStatus();
    await stageChange('sections/intro.tex');
    await vi.advanceTimersByTimeAsync(200);
    expect(calls.gitVerbs).toEqual(['stage sections/intro.tex']);
    expect(git.stagedRows.map((r) => r.path)).toEqual(['sections/intro.tex']);
    expect(git.unstagedRows).toEqual([]);
  });

  it('unstages a path', async () => {
    await unstageChange('sections/intro.tex');
    expect(calls.gitVerbs).toEqual(['unstage sections/intro.tex']);
  });

  it('always asks before discarding, and a "keep" changes nothing', async () => {
    gitOnDisk = { staged: [], unstaged: [modified], conflicted: [] };
    await refreshGitStatus();
    discardAnswer = false;
    await discardChange('sections/intro.tex', false);
    expect(calls.discardQuestions).toEqual(['sections/intro.tex']);
    expect(calls.gitVerbs).toEqual([]);
    expect(git.unstagedRows.map((r) => r.path)).toEqual(['sections/intro.tex']);
  });

  it('says which of the two things a discard did, because only one of them is recoverable', async () => {
    gitOnDisk = { staged: [], unstaged: [modified, untracked], conflicted: [] };
    await refreshGitStatus();
    discardAnswer = true;

    await discardChange('sections/intro.tex', false);
    expect(git.lastDiscard).toBe('sections/intro.tex is back to its last committed version.');

    await discardChange('notes.tex', true);
    // The question said "untracked", so the author agreed to a deletion and not to a restore.
    expect(calls.discardQuestions).toEqual(['sections/intro.tex', 'notes.tex (untracked)']);
    expect(git.lastDiscard).toBe('Deleted notes.tex.');
  });

  it('a Git failure is a sentence in the panel and nowhere else', async () => {
    gitOnDisk = { staged: [], unstaged: [modified], conflicted: [] };
    await refreshGitStatus();
    gitVerbError = '../outside.tex is outside the project';
    await stageChange('../outside.tex');
    expect(git.error).toContain('outside the project');
    expect(app.notice).toBeNull();
  });

  it('switches which view the left pane holds', () => {
    expect(app.activityView).toBe('files');
    showActivityView('source-control');
    expect(app.activityView).toBe('source-control');
    // And the palette reaches the same two actions the chords do (rule 5).
    const ids = allCommands().map((c) => c.id);
    expect(ids).toContain('view-files');
    expect(ids).toContain('view-source-control');
  });
});

describe('committing, and the graph (S10.3b)', () => {
  const staged = { path: 'main.tex', kind: 'modified' as const, renamedFrom: null };

  /** Put a history `n` commits long on the fake disk, with `HEAD` on its newest commit — which
   * is what tells the panel the history has moved (S10.3c). */
  function putHistory(n: number): void {
    commitsOnDisk = history(n);
    branchOnDisk = { ...branchOnDisk, head: commitsOnDisk[0]?.id ?? null };
  }

  /** A history `n` commits long, newest first, as Rust would report it. */
  function history(n: number): CommitRow[] {
    return Array.from({ length: n }, (_unused, i) => ({
      id: `id${i}`,
      shortId: `id${i}`,
      summary: `commit ${i}`,
      author: 'Ada',
      time: 1_760_000_000 - i,
      tags: [],
      wordDelta: 10 - i,
    }));
  }

  it('commits the message, empties the box, and the new row is at the top of the graph', async () => {
    gitOnDisk = { staged: [staged], unstaged: [], conflicted: [] };
    await refreshGitStatus();
    git.message = 'Revised the methods section';
    await commitStaged();
    await vi.advanceTimersByTimeAsync(200);

    expect(calls.gitVerbs).toEqual(['commit Revised the methods section']);
    expect(git.message).toBe('');
    expect(git.stagedRows).toEqual([]);
    expect(git.commits[0]?.summary).toBe('Revised the methods section');
  });

  it('keeps the words in the box when the commit is refused, and says why under it', async () => {
    gitOnDisk = { staged: [staged], unstaged: [], conflicted: [] };
    await refreshGitStatus();
    commitError = 'Git does not know who you are yet.';
    git.message = 'Half a sentence';
    await commitStaged();

    expect(git.error).toContain('Git does not know who you are');
    // The refusal is fixed and the same words tried again — losing them would be the one
    // unrecoverable part of a recoverable mistake.
    expect(git.message).toBe('Half a sentence');
    expect(app.notice).toBeNull();
    expect(git.committing).toBe(false);
  });

  it('asks for one page of history, and offers more only while pages come back full', async () => {
    putHistory(200);
    calls.gitLogPages = []; // the folder opening already read the (then empty) history once
    await refreshGitStatus();
    expect(calls.gitLogPages).toEqual(['0+200']);
    expect(git.commits).toHaveLength(200);
    expect(git.mayHaveMore).toBe(true);

    commitsOnDisk = history(250);
    await loadMoreCommits();
    expect(calls.gitLogPages).toEqual(['0+200', '200+200']);
    expect(git.commits).toHaveLength(250);
    // The second page came back short, so the history has ended and the button goes away.
    expect(git.mayHaveMore).toBe(false);
  });

  it('does not rebuild the graph when only the working tree changed, and keeps its pages when it does', async () => {
    putHistory(400);
    await refreshGitStatus();
    await loadMoreCommits();
    expect(git.commits).toHaveLength(400);

    // A save: the lists move, `HEAD` does not. Rebuilding 400 rows with a word count on each
    // would be ~200 ms of work for an identical answer (S10.3c's measurement).
    calls.gitLogPages = [];
    await refreshGitStatus();
    expect(calls.gitLogPages).toEqual([]);
    expect(git.commits).toHaveLength(400);

    // A commit, or a checkout, or a reset: `HEAD` moves, and then it is re-read at the depth the
    // author had already opened up.
    branchOnDisk = { ...branchOnDisk, head: 'somewhere-else' };
    await refreshGitStatus();
    expect(calls.gitLogPages).toEqual(['0+400']);
  });

  it('carries the branch and its drift for the status bar, and clears both with the project', async () => {
    branchOnDisk = { name: 'thesis', aheadBehind: [2, 1], unborn: false, head: 'head0' };
    await refreshGitStatus();
    expect(git.branch).toEqual({ name: 'thesis', aheadBehind: [2, 1], unborn: false, head: 'head0' });
    expect(git.outgoing).toBe(2);

    gitOnDisk = null; // a project that is not in Git at all
    await refreshGitStatus();
    expect(git.branch).toBeNull();
    expect(git.commits).toEqual([]);
    expect(git.outgoing).toBe(0);
  });
});

describe('amending (S11.1c)', () => {
  const staged = { path: 'main.tex', kind: 'modified' as const, renamedFrom: null };

  function putOneCommit(): void {
    commitsOnDisk = [{ id: 'c1', shortId: 'c1', summary: 'first draft', author: 'Ada', time: 1_760_000_000, tags: [], wordDelta: 5 }];
    branchOnDisk = { ...branchOnDisk, head: 'c1' };
  }

  it('rewrites HEAD in place rather than creating a new commit', async () => {
    putOneCommit();
    gitOnDisk = { staged: [staged], unstaged: [], conflicted: [] };
    await refreshGitStatus();
    git.message = 'a better first message';

    await amendCommit();
    // `amendCommit`, like `commitStaged`, relies on the debounced `git:status-changed` refresh
    // rather than reading the status back itself.
    await vi.advanceTimersByTimeAsync(200);

    expect(calls.gitVerbs).toEqual(['amend a better first message']);
    expect(git.message).toBe('');
    expect(git.commits).toHaveLength(1); // rewritten, not added to
    expect(git.commits[0]?.summary).toBe('a better first message');
  });

  it('keeps the words in the box when the amend is refused, same as a plain commit', async () => {
    putOneCommit();
    await refreshGitStatus();
    commitError = 'A commit needs a message.';
    git.message = 'Half a sentence';

    await amendCommit();

    expect(git.error).toContain('A commit needs a message');
    expect(git.message).toBe('Half a sentence');
    expect(git.committing).toBe(false);
  });
});

describe('syncing (S11.1b)', () => {
  const staged = { path: 'main.tex', kind: 'modified' as const, renamedFrom: null };

  it('Commit & Push commits, then pushes, in that order', async () => {
    gitOnDisk = { staged: [staged], unstaged: [], conflicted: [] };
    await refreshGitStatus();
    git.message = 'Revised the introduction';

    await commitAndPush();

    expect(calls.gitVerbs).toEqual(['commit Revised the introduction', 'push']);
    expect(git.message).toBe('');
    expect(git.committing).toBe(false);
  });

  it('Commit & Sync commits, then syncs, and reports what the sync decided', async () => {
    gitOnDisk = { staged: [staged], unstaged: [], conflicted: [] };
    syncOutcome = { kind: 'pushed', ahead: 1 };
    await refreshGitStatus();
    git.message = 'Revised the introduction';

    await commitAndSync();

    expect(calls.gitVerbs).toEqual(['commit Revised the introduction', 'sync']);
    expect(git.lastSync).toBe('Pushed 1 commit.');
  });

  it('a refused commit never reaches push or sync — one refusal, not two', async () => {
    gitOnDisk = { staged: [staged], unstaged: [], conflicted: [] };
    await refreshGitStatus();
    commitError = 'Git does not know who you are yet.';
    git.message = 'Half a sentence';

    await commitAndPush();

    expect(calls.gitVerbs).toEqual([]);
    expect(git.error).toContain('Git does not know who you are');
    // The same recoverable-mistake rule `commitStaged` already follows: the words survive.
    expect(git.message).toBe('Half a sentence');
  });

  it('the standalone Sync button runs sync alone, with nothing to commit first', async () => {
    branchOnDisk = { ...branchOnDisk, aheadBehind: [0, 1] };
    syncOutcome = { kind: 'fastForwarded', behind: 1 };
    await refreshGitStatus();

    await syncChanges();

    expect(calls.gitVerbs).toEqual(['sync']);
    expect(git.lastSync).toBe('Pulled 1 commit.');
    expect(git.syncing).toBe(false);
  });

  it('a refused sync shows the sentence under the button rather than a dialog', async () => {
    branchOnDisk = { ...branchOnDisk, aheadBehind: [1, 1] };
    await refreshGitStatus();
    syncError = 'Both your computer and the remote have commits the other does not.';

    await syncChanges();

    expect(git.error).toContain('Both your computer and the remote');
    expect(git.lastSync).toBeNull();
    expect(git.syncing).toBe(false);
  });
});

describe('the Git LFS banner (S11.3c)', () => {
  const figure = { path: 'figure.png', sizeBytes: 6_000_000 };

  it('a refresh picks up large-file candidates alongside the three lists', async () => {
    largeFilesOnDisk = [figure];
    await refreshGitStatus();
    expect(git.largeFiles).toEqual([figure]);
    expect(git.visibleLargeFiles).toEqual([figure]);
  });

  it('tracking stages the file through Rust and the banner clears on the next refresh', async () => {
    largeFilesOnDisk = [figure];
    await refreshGitStatus();

    await trackLargeFilesWithLfs(['figure.png']);
    // `gitTrackWithLfs`'s fake emits the same `git:status-changed` event the real command does,
    // which only schedules a refresh (`scheduleGitRefresh`'s own 120 ms coalescing window) —
    // advancing past it is what makes the clearing a real refresh and not this test's bookkeeping.
    await vi.advanceTimersByTimeAsync(120);

    expect(calls.gitVerbs).toEqual(['trackWithLfs figure.png']);
    expect(git.largeFiles).toEqual([]);
    expect(git.trackingLfs).toBe(false);
  });

  it('a refusal — usually Git LFS not being installed — shows under the box, and the candidate stays', async () => {
    largeFilesOnDisk = [figure];
    await refreshGitStatus();
    lfsError = "Git LFS isn't installed on this machine. Install it from https://git-lfs.com, then try again.";

    await trackLargeFilesWithLfs(['figure.png']);

    expect(git.error).toContain("isn't installed");
    expect(git.largeFiles).toEqual([figure]);
  });

  it('dismissing hides the candidate without tracking it, for this session only', async () => {
    largeFilesOnDisk = [figure];
    await refreshGitStatus();

    dismissLargeFiles(['figure.png']);

    expect(git.visibleLargeFiles).toEqual([]);
    // Nothing was actually tracked — the file is still a candidate on the next real refresh.
    expect(git.largeFiles).toEqual([figure]);
    expect(calls.gitVerbs).toEqual([]);
  });
});

describe('resolving a conflict (S11.2b)', () => {
  // Not `main.tex`: `openFolder` in `beforeEach` already opened the project's root file before
  // this test gets to set the disk content it wants, which would make `openFile` below a no-op
  // short-circuit against a buffer that still holds the `beforeEach` default.
  const conflictedMarkers = '<<<<<<< HEAD\nmine\n=======\ntheirs\n>>>>>>> origin/main\n';

  it('writes the resolution, stages it, and reopens the file clean rather than from the stale buffer', async () => {
    disk.set('notes.tex', conflictedMarkers);
    gitOnDisk = { staged: [], unstaged: [], conflicted: [{ path: 'notes.tex', kind: 'conflicted', renamedFrom: null }] };
    await refreshGitStatus();
    await openFile('notes.tex');
    // The buffer `openFile` made really does hold the raw markers nobody is meant to see — this
    // is the hazard `resolveMergeConflict` exists to clean up, not a thing anyone reads on screen.
    expect(app.activeDoc!.text()).toBe(conflictedMarkers);

    await resolveMergeConflict('notes.tex', 'the resolved sentence\n');

    expect(calls.writes).toContainEqual({ path: 'notes.tex', contents: 'the resolved sentence\n' });
    expect(calls.gitVerbs).toContain('stage notes.tex');
    expect(app.activePath).toBe('notes.tex');
    expect(app.activeDoc!.text()).toBe('the resolved sentence\n');
  });

  it('discards the stale buffer without disturbing whatever tab is actually active', async () => {
    disk.set('notes.tex', conflictedMarkers);
    gitOnDisk = { staged: [], unstaged: [], conflicted: [{ path: 'notes.tex', kind: 'conflicted', renamedFrom: null }] };
    await refreshGitStatus();
    await openFile('notes.tex');
    await openFile('main.tex'); // notes.tex's tab stays open, just no longer the active one

    await resolveMergeConflict('notes.tex', 'the resolved sentence\n');

    expect(app.activePath).toBe('main.tex');
    expect(app.openTabs).not.toContain('notes.tex');
  });
});

describe("the writer's own additions (S10.3c)", () => {
  const staged = { path: 'main.tex', kind: 'modified' as const, renamedFrom: null };

  beforeEach(() => {
    proseOnDisk = { wordsBefore: 1200, wordsAfter: 1440, sections: ['Methods'], paths: ['main.tex'], addedPaths: [] };
  });

  it('fills the empty box with a sentence built from the outline and the diff', async () => {
    await refreshGitStatus();
    expect(git.message).toBe('Revised Methods, +240 words');
    expect(git.wordsSinceCommit).toBe(240);
  });

  it('never overwrites the author once they have typed in it', async () => {
    await refreshGitStatus();
    git.message = 'Reply to reviewer 2';
    claimCommitMessage();

    // Another save, another refresh, a different suggestion — and the box is left alone.
    proseOnDisk = { ...proseOnDisk, wordsAfter: 1600, sections: ['Results'] };
    await refreshGitStatus();
    expect(git.message).toBe('Reply to reviewer 2');
    // The number still updates, because it is a fact and not a sentence.
    expect(git.wordsSinceCommit).toBe(400);
  });

  it('starts suggesting again after a commit, since the old sentence described the old work', async () => {
    gitOnDisk = { staged: [staged], unstaged: [], conflicted: [] };
    await refreshGitStatus();
    git.message = 'Mine';
    claimCommitMessage();
    await commitStaged();
    await vi.advanceTimersByTimeAsync(200);
    expect(calls.gitVerbs).toEqual(['commit Mine']);

    proseOnDisk = { wordsBefore: 0, wordsAfter: 30, sections: [], paths: ['notes.tex'], addedPaths: ['notes.tex'] };
    await refreshGitStatus();
    expect(git.message).toBe('Added notes.tex, +30 words');
  });

  it('forgets the previous project’s half-written message when a folder opens', async () => {
    git.message = 'About the other paper';
    claimCommitMessage();
    await openFolder('/proj');
    await vi.advanceTimersByTimeAsync(0);
    expect(git.message).toBe('Revised Methods, +240 words');
  });
});

describe('signing in to GitHub (S10.4b)', () => {
  it('asks once at startup who is signed in, and never again on a timer', async () => {
    expect(calls.githubCalls).toEqual(['account']);
    await vi.advanceTimersByTimeAsync(30_000);
    expect(calls.githubCalls).toEqual(['account']);
  });

  it('starts the flow and shows the code the event brings', async () => {
    await signInToGitHub();
    expect(calls.githubCalls).toContain('signIn');
    expect(github.stage).toBe('starting');

    signInHandler({
      stage: 'code',
      userCode: 'WDJB-MJHT',
      verificationUri: 'https://github.com/login/device',
      expiresInSeconds: 900,
    });
    expect(github.stage).toBe('waiting');
    expect(github.code?.userCode).toBe('WDJB-MJHT');
  });

  it('will not start a second sign-in while one is waiting', async () => {
    await signInToGitHub();
    signInHandler({ stage: 'code', userCode: 'A', verificationUri: 'B', expiresInSeconds: 900 });
    calls.githubCalls = [];
    await signInToGitHub();
    expect(calls.githubCalls).toEqual([]);
  });

  it('a build with no OAuth app says so instead of offering a button that cannot work', async () => {
    signInError = 'Signing in to GitHub is not configured in this build of Abstract-Tex.';
    await signInToGitHub();
    expect(github.stage).toBe('idle');
    expect(github.error).toContain('not configured');
    expect(app.notice).toBeNull(); // nothing else in the app waits on an account
  });

  it('opens the verification page in the browser, and the URL is on screen either way', async () => {
    await signInToGitHub();
    signInHandler({ stage: 'code', userCode: 'A', verificationUri: 'https://github.com/login/device', expiresInSeconds: 900 });
    await openVerificationPage();
    expect(calls.openedUrls).toEqual(['https://github.com/login/device']);
    expect(github.code?.verificationUri).toBe('https://github.com/login/device');
  });

  it('cancelling stops the polling and shows no failure', async () => {
    await signInToGitHub();
    signInHandler({ stage: 'code', userCode: 'A', verificationUri: 'B', expiresInSeconds: 900 });
    await cancelGitHubSignIn();
    expect(calls.githubCalls).toContain('cancel');
    expect(github.code).toBeNull();
    expect(github.error).toBeNull();

    // The event Rust sends after a cancel must not put a failure back on screen.
    signInHandler({ stage: 'failed', message: 'The sign-in was cancelled.', cancelled: true });
    expect(github.error).toBeNull();
  });

  it('signing out forgets the account here as well as in the keychain', async () => {
    signInHandler({ stage: 'signedIn', login: 'ada' });
    expect(github.account).toEqual({ login: 'ada' });
    await signOutOfGitHub();
    expect(calls.githubCalls).toContain('signOut');
    expect(github.account).toBeNull();
  });
});

describe('making a folder a repository (S10.5a)', () => {
  it('turns a folder with no Git into one, and the panel stops saying there is none', async () => {
    gitOnDisk = null;
    await refreshGitStatus();
    expect(git.isRepository).toBe(false);

    await initialiseRepository();
    expect(calls.gitVerbs).toEqual(['initialise']);
    expect(git.isRepository).toBe(true);
    expect(git.needsIdentity).toBe(false);
    expect(git.initialising).toBe(false);
  });

  it('says what to do when Git does not know who the author is', async () => {
    gitOnDisk = null;
    initialiseNeedsIdentity = true;
    await initialiseRepository();
    // The repository exists either way; what is missing is a name to sign with (S10.2b).
    expect(git.isRepository).toBe(true);
    expect(git.needsIdentity).toBe(true);
  });

  it('a folder already inside a repository is refused with the sentence, and nothing changes', async () => {
    gitOnDisk = null;
    initialiseError = 'This folder is already inside a Git repository (/home/ada/thesis).';
    await initialiseRepository();
    expect(git.error).toContain('already inside a Git repository');
    expect(git.isRepository).toBe(false);
    expect(app.notice).toBeNull();
  });

  it('offers the .gitignore line for a repository that predates this app, and adds it only when asked', async () => {
    ourFolderIgnoredOnDisk = false;
    await refreshGitStatus();
    expect(git.ourFolderIsIgnored).toBe(false);
    // Nothing has been written yet: the offer is a button, not an action taken on their file.
    expect(calls.gitVerbs).toEqual([]);

    await ignoreOurFolder();
    expect(calls.gitVerbs).toEqual(['ignoreOurFolder']);
    expect(git.ourFolderIsIgnored).toBe(true);
  });

  it('asks nothing about ignore rules where there is no repository to ask about', async () => {
    gitOnDisk = null;
    await refreshGitStatus();
    expect(git.ourFolderIsIgnored).toBeNull();
  });
});

describe("a refusal outlives the refresh that follows it (S10.5a's bug)", () => {
  it('keeps a verb’s sentence on screen after the status has been read again', async () => {
    gitOnDisk = { staged: [], unstaged: [{ path: 'main.tex', kind: 'modified', renamedFrom: null }], conflicted: [] };
    await refreshGitStatus();
    gitVerbError = 'main.tex could not be staged.';

    await stageChange('main.tex');
    expect(git.error).toContain('could not be staged');

    // Every verb is followed by a refresh — the event, or an awaited one. Before the fix, this
    // read cleared the slot and the sentence vanished a moment after appearing.
    gitStatusHandler();
    await vi.advanceTimersByTimeAsync(300);
    expect(git.error).toContain('could not be staged');

    // The next verb is what clears it, because that is the next thing the author did.
    gitVerbError = null;
    await stageChange('main.tex');
    expect(git.error).toBeNull();
  });

  it('a read failure is its own slot, and clears itself when reading works again', async () => {
    const failing = () => {
      throw new Error('this repository could not be read');
    };
    const original = ipc.gitStatus;
    (ipc as { gitStatus: typeof ipc.gitStatus }).gitStatus = failing as typeof ipc.gitStatus;
    await refreshGitStatus();
    expect(git.readError).toContain('could not be read');

    (ipc as { gitStatus: typeof ipc.gitStatus }).gitStatus = original;
    await refreshGitStatus();
    expect(git.readError).toBeNull();
  });
});

describe('creating the repository on GitHub (S10.5b)', () => {
  beforeEach(() => {
    accountOnDisk = { login: 'ada' };
  });

  it('creates a private one without asking anything, and sets it as origin', async () => {
    await createGitHubRepository('thesis', false);
    expect(calls.created).toEqual(['thesis private']);
    // Private needs no confirmation — it is what the app does unless told otherwise.
    expect(calls.publicQuestions).toEqual([]);
    expect(github.created?.fullName).toBe('ada/thesis');
    expect(github.created?.private).toBe(true);
    expect(git.originUrl).toBe('https://github.com/ada/thesis.git');
  });

  it('asks before a public one, and a "keep it private" creates nothing at all', async () => {
    publicAnswer = false;
    await createGitHubRepository('thesis', true);
    expect(calls.publicQuestions).toEqual(['thesis']);
    // Not a private repository they did not ask for: they chose public and were talked out of
    // it, and quietly doing something else would be the app deciding for them.
    expect(calls.created).toEqual([]);
    expect(github.created).toBeNull();
    expect(git.originUrl).toBeNull();
  });

  it('creates a public one once the confirmation is answered yes', async () => {
    publicAnswer = true;
    await createGitHubRepository('open-paper', true);
    expect(calls.publicQuestions).toEqual(['open-paper']);
    expect(calls.created).toEqual(['open-paper public']);
    expect(github.created?.private).toBe(false);
  });

  it('trims the name, and refuses an empty one before asking GitHub', async () => {
    await createGitHubRepository('   ', false);
    expect(calls.created).toEqual([]);
    expect(github.createError).toBe('A repository needs a name.');

    github.createError = null;
    await createGitHubRepository('  thesis  ', false);
    expect(calls.created).toEqual(['thesis private']);
  });

  it('a name GitHub will not take is a sentence in the panel, and nothing was set as origin', async () => {
    createError = 'GitHub said: Repository creation failed. name already exists on this account';
    await createGitHubRepository('thesis', false);
    expect(github.createError).toContain('already exists');
    expect(github.created).toBeNull();
    expect(git.originUrl).toBeNull();
    expect(app.notice).toBeNull();
  });

  it('will not create two at once', async () => {
    github.creating = true;
    await createGitHubRepository('thesis', false);
    expect(calls.created).toEqual([]);
    github.creating = false;
  });
});


describe('comparing two commits (S11.4d)', () => {
  const finishedDiff = (generation: number): CompileEvent => ({
    status: 'finished',
    generation,
    success: true,
    pdfPath: '/proj/.abstract-tex/latexdiff/x/build/main.pdf',
    logPath: null,
    diagnostics: [],
    durationMs: 900,
    stderr: '',
  });

  const row = (id: string): CommitRow => ({
    id,
    shortId: id.slice(0, 7),
    summary: id,
    author: 'Ada',
    time: 1_000,
    tags: [],
    wordDelta: 0,
  });

  beforeEach(() => {
    clearComparison();
    git.commits = [row('c3c3c3c3'), row('b2b2b2b2'), row('a1a1a1a1')];
  });

  it('asks for a comparison on the second mark, and shows its PDF when the build finishes', async () => {
    markGraphRow('a1a1a1a1');
    expect(comparisons.asked).toEqual([]);
    markGraphRow('c3c3c3c3');
    await vi.advanceTimersByTimeAsync(0);
    expect(comparisons.asked).toEqual([['a1a1a1a1', 'c3c3c3c3']]);
    expect(compare.view.phase).toBe('building');

    diffHandler(finishedDiff(1));
    expect(compare.view.phase).toBe('ready');
    expect(compare.view.pdfUrl).toContain('asset:///proj/.abstract-tex/latexdiff/x/build/main.pdf');
  });

  it('keeps the comparison out of the live build: no event of the diff lane touches `app.compile`', async () => {
    const before = app.compile;
    markGraphRow('a1a1a1a1');
    markGraphRow('c3c3c3c3');
    await vi.advanceTimersByTimeAsync(0);
    diffHandler(finishedDiff(1));
    expect(app.compile).toBe(before);
  });

  it('opens the drawer on a comparison that failed, and leaves the live diagnostics alone', async () => {
    markGraphRow('a1a1a1a1');
    markGraphRow('c3c3c3c3');
    await vi.advanceTimersByTimeAsync(0);
    app.drawerOpen = false;
    diffHandler({ ...finishedDiff(1), success: false, pdfPath: null, diagnostics: [], stderr: 'x' } as CompileEvent);
    expect(compare.view.phase).toBe('failed');
    expect(app.drawerOpen).toBe(true);
    expect(app.compile.diagnostics).toEqual([]);
  });

  it('replays events that arrived before the answer named the generation', async () => {
    let answer!: (value: ComparisonStarted) => void;
    compareAnswer = () => new Promise((resolve) => (answer = resolve));
    markGraphRow('a1a1a1a1');
    markGraphRow('c3c3c3c3');
    await vi.advanceTimersByTimeAsync(0);

    diffHandler({ status: 'progress', generation: 4, message: 'Downloading…' });
    diffHandler(finishedDiff(4));
    expect(compare.view.phase).toBe('building'); // the generation is not known yet

    answer({ status: 'building', older: 'a1a1a1a1', newer: 'c3c3c3c3', generation: 4 });
    await vi.advanceTimersByTimeAsync(0);
    expect(compare.view.phase).toBe('ready');
  });

  it('lets only the latest request speak when two answers cross', async () => {
    const answers: Array<(value: ComparisonStarted) => void> = [];
    compareAnswer = () => new Promise((resolve) => answers.push(resolve));

    markGraphRow('a1a1a1a1');
    markGraphRow('b2b2b2b2'); // first request
    await vi.advanceTimersByTimeAsync(0);
    markGraphRow('a1a1a1a1'); // a third click starts over…
    markGraphRow('c3c3c3c3'); // …and this is the second request
    await vi.advanceTimersByTimeAsync(0);
    expect(answers).toHaveLength(2);

    answers[1]!({ status: 'building', older: 'a1a1a1a1', newer: 'c3c3c3c3', generation: 2 });
    await vi.advanceTimersByTimeAsync(0);
    answers[0]!({ status: 'building', older: 'a1a1a1a1', newer: 'b2b2b2b2', generation: 1 });
    await vi.advanceTimersByTimeAsync(0);

    expect(compare.view.generation).toBe(2);
    expect(compare.view.newer).toBe('c3c3c3c3');
  });

  it('shows a refusal under the Graph, goes back to the live PDF and clears the marks', async () => {
    compareAnswer = async () => {
      throw new Error("latexdiff isn't installed on this machine.");
    };
    markGraphRow('a1a1a1a1');
    markGraphRow('c3c3c3c3');
    await vi.advanceTimersByTimeAsync(0);
    expect(compare.refusal).toContain("latexdiff isn't installed");
    expect(compare.mode).toBe(false);
    expect(compare.marks).toEqual({ from: null, to: null });
  });

  it('says nothing for a comparison Rust says was superseded', async () => {
    compareAnswer = async () => ({ status: 'superseded' });
    markGraphRow('a1a1a1a1');
    markGraphRow('c3c3c3c3');
    await vi.advanceTimersByTimeAsync(0);
    expect(compare.refusal).toBeNull();
    expect(compare.view.phase).toBe('building');
  });

  it('shows a pair whose PDF is already on disk at once', async () => {
    compareAnswer = async (a, b) => ({ status: 'ready', older: a, newer: b, pdfPath: '/proj/.abstract-tex/latexdiff/x/build/main.pdf' });
    markGraphRow('a1a1a1a1');
    markGraphRow('c3c3c3c3');
    await vi.advanceTimersByTimeAsync(0);
    expect(compare.view.phase).toBe('ready');
  });

  it('Back to live PDF leaves diff mode, clears the marks and ignores an answer still on its way', async () => {
    let answer!: (value: ComparisonStarted) => void;
    compareAnswer = () => new Promise((resolve) => (answer = resolve));
    markGraphRow('a1a1a1a1');
    markGraphRow('c3c3c3c3');
    await vi.advanceTimersByTimeAsync(0);

    clearComparison();
    expect(compare.mode).toBe(false);
    expect(compare.marks).toEqual({ from: null, to: null });

    answer({ status: 'building', older: 'a1a1a1a1', newer: 'c3c3c3c3', generation: 1 });
    await vi.advanceTimersByTimeAsync(0);
    expect(compare.mode).toBe(false);
  });

  it('the palette entry compares the last two commits, newest as the end', async () => {
    expect(allCommands().some((c) => c.id === 'compare-previous-commit')).toBe(true);
    compareWithPreviousCommit();
    await vi.advanceTimersByTimeAsync(0);
    expect(comparisons.asked).toEqual([['b2b2b2b2', 'c3c3c3c3']]);
    expect(app.activityView).toBe('source-control');
  });

  it('the palette entry says so when there is no earlier commit', () => {
    git.commits = [row('c3c3c3c3')];
    compareWithPreviousCommit();
    expect(comparisons.asked).toEqual([]);
    expect(compare.refusal).toMatch(/no earlier commit/);
  });

  it('Save as… writes where the author chose, and does nothing once they cancel', async () => {
    compareAnswer = async (a, b) => ({ status: 'ready', older: a, newer: b, pdfPath: '/proj/x.pdf' });
    markGraphRow('a1a1a1a1');
    markGraphRow('c3c3c3c3');
    await vi.advanceTimersByTimeAsync(0);

    await saveComparisonAs();
    expect(comparisons.saved).toEqual(['/out/diff.pdf']);

    savePathAnswer = null;
    await saveComparisonAs();
    expect(comparisons.saved).toEqual(['/out/diff.pdf']);
  });

  it('opening another folder forgets the comparison and its marks', async () => {
    markGraphRow('a1a1a1a1');
    markGraphRow('c3c3c3c3');
    await vi.advanceTimersByTimeAsync(0);
    await openFolder('/proj');
    expect(compare.mode).toBe(false);
    expect(compare.marks).toEqual({ from: null, to: null });
  });
});


describe('cloning a repository (S11.5b)', () => {
  const thesis: GitHubRepository = {
    fullName: 'ada/thesis',
    cloneUrl: 'https://github.com/ada/thesis.git',
    htmlUrl: 'https://github.com/ada/thesis',
    private: true,
  };

  beforeEach(() => {
    closeCloneWindow();
    clone.url = '';
    clone.folderName = '';
    clone.repositories = null;
    clone.error = null;
    clone.listError = null;
    github.account = null;
  });

  it('opens with an address field and no account call when nobody is signed in', () => {
    showCloneWindow();
    expect(clone.visible).toBe(true);
    expect(clones.lists).toBe(0);
  });

  it('lists the account’s repositories when somebody is signed in', async () => {
    github.account = { login: 'ada' };
    listAnswer = [thesis];
    showCloneWindow();
    await vi.advanceTimersByTimeAsync(0);
    expect(clone.repositories).toEqual([thesis]);
  });

  it('says why a listing failed and does not keep a stale list', async () => {
    github.account = { login: 'ada' };
    clone.repositories = [thesis];
    listAnswer = new Error('GitHub no longer accepts this sign-in. Signing in again fixes it.');
    await loadCloneList();
    expect(clone.listError).toContain('no longer accepts');
    expect(clone.repositories).toBeNull();
    expect(clone.listing).toBe(false);
  });

  it('choosing a row fills in the address and clears a typed folder name', () => {
    clone.folderName = 'old';
    chooseRepositoryToClone(thesis);
    expect(clone.url).toBe(thesis.cloneUrl);
    expect(clone.folderName).toBe('');
  });

  it('clones into the chosen folder and opens the result as a project', async () => {
    clone.url = ' https://github.com/ada/thesis.git ';
    showCloneWindow();
    await cloneRepository();
    expect(clones.asked).toEqual([['https://github.com/ada/thesis.git', '/papers', null]]);
    expect(clone.visible).toBe(false);
    expect(clone.url).toBe('');
    expect(app.project?.rootDir).toBe('/proj'); // the fake `openProject` answers with /proj
  });

  it('passes a typed folder name on, and an empty one as null', async () => {
    clone.url = 'https://github.com/ada/thesis.git';
    clone.folderName = '  draft  ';
    await cloneRepository();
    expect(clones.asked[0]![2]).toBe('draft');
  });

  it('does nothing for an empty address, and nothing once the folder picker is cancelled', async () => {
    await cloneRepository();
    expect(clones.asked).toEqual([]);

    clone.url = 'https://github.com/ada/thesis.git';
    parentFolderAnswer = null;
    await cloneRepository();
    expect(clones.asked).toEqual([]);
    expect(clone.cloning).toBe(false);
    expect(clone.error).toBeNull();
  });

  it('keeps the window open with the sentence when the clone is refused', async () => {
    showCloneWindow();
    clone.url = 'https://github.com/ada/thesis.git';
    cloneAnswer = new Error('/papers/thesis already has files in it, so nothing was downloaded there.');
    await cloneRepository();
    expect(clone.visible).toBe(true);
    expect(clone.error).toContain('already has files');
    expect(clone.cloning).toBe(false);
    expect(clone.url).toBe('https://github.com/ada/thesis.git');
  });

  it('is in the palette', () => {
    expect(allCommands().some((c) => c.id === 'clone-repository' && c.title === 'Clone a repository…')).toBe(true);
  });
});


describe('snapshots (S11.6)', () => {
  const newest: SnapshotRow = { id: 'b'.repeat(40), shortId: 'bbbbbbb', time: 2_000, words: 120 };
  const older: SnapshotRow = { id: 'a'.repeat(40), shortId: 'aaaaaaa', time: 1_000, words: 80 };

  beforeEach(() => {
    closeSnapshot();
    snapshots.rows = null;
    snapshots.restored = null;
    snapshots.viewError = null;
    snapshotRows = [newest, older];
    snapshotFilesOnDisk = {
      [newest.id]: { 'chapters/a.tex': 'newer chapter', 'main.tex': 'newer main' },
      [older.id]: { 'main.tex': 'older main' },
    };
  });

  it('lists the snapshots when the Source Control view comes up', async () => {
    showActivityView('source-control');
    await vi.advanceTimersByTimeAsync(0);
    expect(snapshots.rows).toEqual([newest, older]);
  });

  it('says why the list could not be read, without a notice', async () => {
    snapshotError = 'the snapshot could not be read back';
    await refreshSnapshots();
    expect(snapshots.listError).toContain('could not be read back');
    expect(app.notice).toBeNull();
  });

  it('opening a snapshot shows the root file first, read-only', async () => {
    await openSnapshot(newest);
    expect(snapshots.files).toEqual(['chapters/a.tex', 'main.tex']);
    expect(snapshots.filePath).toBe('main.tex'); // the fake project's root file
    expect(snapshots.text).toBe('newer main');
  });

  it('shows another file of the same snapshot on request', async () => {
    await openSnapshot(newest);
    await showSnapshotFile('chapters/a.tex');
    expect(snapshots.text).toBe('newer chapter');
  });

  it('does not let a slow answer for one snapshot land in another', async () => {
    const first = openSnapshot(newest);
    await openSnapshot(older);
    await first;
    expect(snapshots.selected).toBe(older);
    expect(snapshots.text).toBe('older main');
  });

  it('restoring asks Rust for exactly the file on screen and says the replaced version is kept', async () => {
    await openSnapshot(older);
    await restoreSnapshotFile();
    expect(restores).toEqual([[older.id, 'main.tex']]);
    expect(snapshots.restored).toMatch(/Restored main\.tex\..*kept/);
    expect(snapshots.restoring).toBe(false);
  });

  it('a refused restore is a sentence in the viewer and claims nothing was restored', async () => {
    await openSnapshot(older);
    snapshotError = 'The current version could not be kept first, so nothing was changed: disk full';
    await restoreSnapshotFile();
    expect(snapshots.viewError).toContain('nothing was changed');
    expect(snapshots.restored).toBeNull();
  });

  it('will not restore before the text is on screen, nor while a restore is running', async () => {
    snapshots.selected = older;
    snapshots.filePath = 'main.tex';

    snapshots.text = null;
    await restoreSnapshotFile();
    expect(restores).toEqual([]);

    snapshots.text = 'older main';
    snapshots.restoring = true;
    await restoreSnapshotFile();
    expect(restores).toEqual([]);

    snapshots.restoring = false;
    await restoreSnapshotFile();
    expect(restores).toEqual([[older.id, 'main.tex']]);
  });

  it('is in the palette', () => {
    expect(allCommands().some((c) => c.id === 'show-snapshots' && c.title === 'Recover an earlier version…')).toBe(true);
  });

  it('forgets one project’s snapshots when another folder opens', async () => {
    snapshots.rows = [newest];
    snapshots.selected = newest;
    snapshotRows = [];
    await openFolder('/proj');
    await vi.advanceTimersByTimeAsync(0);
    expect(snapshots.selected).toBeNull();
    expect(snapshots.rows).toEqual([]);
  });
});


describe('the side-by-side view of a Changes row (S11.7)', () => {
  beforeEach(() => closeDiff());

  it('opens a .tex row side by side with the two texts Rust sent', async () => {
    diffAnswer = { before: 'old text', after: 'new text' };
    await openChangeRow('chapters/a.tex', false, false);
    expect(diffsAsked).toEqual([['chapters/a.tex', false]]);
    expect(diffView.open).toEqual({ path: 'chapters/a.tex', staged: false, deleted: false });
    expect(diffView.sides).toEqual({ before: 'old text', after: 'new text' });
  });

  it('asks for the staged pair for a Staged Changes row, and for a .bib too', async () => {
    await openChangeRow('refs.bib', true, false);
    expect(diffsAsked).toEqual([['refs.bib', true]]);
  });

  it('opens any other file as before, and a deleted one not at all', async () => {
    await openChangeRow('figure.png', false, false);
    expect(diffView.open).toBeNull();
    expect(diffsAsked).toEqual([]);

    await openChangeRow('figure.png', false, true);
    expect(diffView.open).toBeNull();
  });

  it('shows a deleted .tex file as a pure removal and offers no Open file', async () => {
    diffAnswer = { before: 'what it said', after: '' };
    await openChangeRow('gone.tex', false, true);
    expect(diffView.open?.deleted).toBe(true);
    expect(diffView.sides?.after).toBe('');
    await openDiffedFile();
    expect(diffView.open).not.toBeNull(); // nothing to open: the view stays
  });

  it('says why a file could not be shown, and keeps the view open for the sentence', async () => {
    diffAnswer = new Error('main.tex is not a text file, so there is no side-by-side view of it.');
    await openChangeRow('main.tex', false, false);
    expect(diffView.error).toContain('not a text file');
    expect(diffView.sides).toBeNull();
    expect(diffView.open).not.toBeNull();
  });

  it('drops a slow answer that belongs to a row no longer open', async () => {
    const first = openChangeRow('a.tex', false, false);
    diffAnswer = { before: 'b-old', after: 'b-new' };
    await openChangeRow('b.tex', false, false);
    await first;
    expect(diffView.open?.path).toBe('b.tex');
    expect(diffView.sides).toEqual({ before: 'b-old', after: 'b-new' });
  });

  it('Open file closes the view and opens the file in the editor', async () => {
    disk.set('chapters/a.tex', 'chapter text');
    await openChangeRow('chapters/a.tex', false, false);
    await openDiffedFile();
    expect(diffView.open).toBeNull();
    expect(app.activePath).toBe('chapters/a.tex');
  });
});
