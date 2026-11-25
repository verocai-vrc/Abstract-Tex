// Every sentence of the start screen (S17.4), in one place and in English, so that a translation has
// one file to translate. Plain strings, and functions where something is filled in. No component of
// the start screen has words of its own (`src/components/start.test.ts` sees to it).
//
// Who reads these: people who have only used Word. The thing they work on is a *document*; the folder
// it is kept in is small print. Nothing here says project, repository or compile.

import type { Colours, UnpackRefusal } from './ipc';

export const words = {
  appName: 'Abstract',
  /** The name of the list down the left, for a screen reader. */
  startNavigation: 'Start',

  pages: { home: 'Home', new: 'New', open: 'Open', look: 'Text and colours', help: 'Help' },

  helpTitle: 'Help',
  help: [
    {
      question: 'Where are my documents?',
      answer:
        'On this computer, as ordinary files. Each document is a folder of its own, and the ones you start here go in a folder called Abstract-Tex, inside your Documents folder, unless you chose another place with “Change…” on the New page. “Show in folder”, beside a document on Home, opens the folder it is in.',
    },
    {
      question: 'How do I get a PDF, or print?',
      answer:
        'The PDF is made again each time you stop typing, and shown beside your text. “Save PDF…”, at the top of the window, puts a copy where you choose. Press “Open it” in the message that follows and your usual PDF program shows it; print it from there.',
    },
    {
      question: 'Is my work saved?',
      answer:
        'Yes. About a second after you stop typing, your words are in the file on this computer. There is nothing to press, though Ctrl S (⌘ S on a Mac) saves at once. A dot beside a file’s name is there until its last words are saved. If a file cannot be written, a message says so and Abstract keeps trying. There is no copy anywhere else unless you make one, so keep a copy of your documents as you would of any files.',
    },
    {
      question: 'How do I get back an earlier version?',
      answer:
        'A version is kept each time the document becomes a PDF. With the document open, press Ctrl K (⌘ K on a Mac), type “earlier version” and press Enter; or press Source Control, the second button down the left edge, and look under Snapshots. Press a version to read it. “Restore this file” puts that file back as it was, and the version it replaces is kept too.',
    },
    {
      question: 'Can I open a Word file?',
      answer:
        'No. Start a new document here, copy the text in Word, and paste it in. Words and paragraphs come along; headings, footnotes, tables and pictures do not, and are made again here.',
    },
    {
      question: 'Why does the first PDF take minutes?',
      answer:
        'Abstract makes the PDF with LaTeX, which has thousands of parts, and fetches only the ones your document uses. The first time that is about 40 MB, and it needs the internet. They are kept, so from then on a PDF takes seconds and needs no internet, until a document uses a part that has not been fetched yet.',
    },
    {
      question: 'What leaves my computer?',
      answer:
        'Nothing you write, until you ask for it. Abstract fetches parts of LaTeX, so the server they come from can see which ones. The rest is sent only when you do something that sends it: signing in to GitHub, getting a document from a Git address or sending it there, sharing a document live, using the assistant (off until you set it up and switch it on), pasting the DOI, arXiv number or ISBN of a reference, which is looked up, opening a link, or reporting a problem. If you have signed in to GitHub, Abstract also asks GitHub whose account it is each time it starts, until you sign out. Abstract has no accounts of its own and counts nothing.',
    },
    {
      question: 'Where can I ask something else?',
      answer:
        'At github.com/verocai-vrc/Abstract-Tex/issues. Anyone can read what is asked there, so leave out what is in your document. Asking needs a GitHub account, which is free.',
    },
  ],

  /** Where the PDF will be, while the first one is made (S17.14). */
  firstPdf:
    'Getting your document ready. The first time, Abstract fetches the parts of LaTeX it needs: a few minutes and about 40 MB, once. You can start writing now.',
  firstPdfSoFar: (parts: number, seconds: number) => {
    const minutes = Math.floor(seconds / 60);
    const time = minutes === 0 ? `${seconds} s` : `${minutes} min ${seconds % 60} s`;
    return `${parts === 1 ? '1 part' : `${parts} parts`} so far, ${time}`;
  },
  needsInternetOnce:
    'Your document is safe, but its first PDF needs the internet, once, to fetch the parts of LaTeX it uses. Abstract cannot reach it now.',
  tryAgain: 'Try again',

  /** Where there is room for one pane (S17.13). */
  panesLabel: 'What to show',
  panes: { files: 'Files', write: 'Write', pdf: 'PDF' },

  /** Over an open document: the way back to it. */
  backTo: (title: string) => `Back to ${title}`,
  /** Of the open document, whose own question is out of sight. */
  changedElsewhere: (file: string) =>
    `Something else changed ${file} while it had words of yours that are not saved. Go back to the document and choose which of the two to keep.`,
  openNow: 'Open now',

  savePdf: 'Save PDF…',
  savePdfTitle: 'Save the PDF',
  savingPdf: 'Getting the newest PDF ready to save…',
  savedPdf: (name: string, place: string) => `Saved ${name} in ${place}.`,
  savedOlderPdf: (name: string, place: string) =>
    `Saved ${name} in ${place}. It is an earlier version: your newest text could not be made into a PDF.`,
  noPdfForProblems: 'There is no PDF yet. Correct what the list of problems says first.',
  noPdf: 'There is no PDF yet, so nothing was saved.',
  pdfNotKept:
    'The PDF could not be saved: no copy of it could be kept in the document\'s folder. The disk may be full, or another program may be using that copy. Nothing was saved.',
  openIt: 'Open it',
  dismiss: 'Close this message',

  welcome: 'Welcome to Abstract',
  welcomeLine: 'Choose what you want to write. You get a finished-looking page to write over, and a PDF of it.',
  openOneYouHave: 'Open a document you already have',
  openButton: 'Open a document…',

  carryOn: 'Carry on',
  open: 'Open',
  startSomethingNew: 'Start something new',
  seeAll: (count: number) => `See all ${count}`,
  templatesCannotBeShown: 'The list of documents to start from could not be read.',

  recentDocuments: 'Recent documents',
  searchRecents: 'Search recent documents',
  search: 'Search',
  noRecentMatches: (query: string) => `No recent document matches “${query}”. Try fewer words.`,
  homeFolder: 'Home folder',
  notFound: 'Not found where it was. It may be on a drive that isn’t connected.',
  findIt: 'Find it…',
  showInFolder: 'Show in folder',
  takeOffList: 'Take off this list',
  nothingIsDeleted: 'Nothing is deleted.',
  clearList: 'Clear this list',
  clearListQuestion: (count: number) =>
    count === 1 ? 'Take the one document off this list? Nothing is deleted.' : `Take all ${count} documents off this list? Nothing is deleted.`,
  clearListYes: 'Yes, clear it',
  clearListNo: 'No, keep it',
  /** A row or card, said whole: title, where it is kept, when it was last used. */
  row: (title: string, place: string, when: string) => `${title}, ${place}, ${when}`,
  /** An action on one row: what it does, then to which document. */
  onDocument: (action: string, title: string) => `${action}: ${title}`,

  newTitle: 'New document',
  newTitleOf: (template: string) => `New document: ${template}`,
  searchTemplates: 'Search',
  searchTemplatesLabel: 'Search the kinds of document',
  kindOfDocument: 'Kind of document',
  noTemplateMatches: (query: string) => `Nothing matches “${query}”. Try fewer words.`,
  previewOf: (template: string) => `First page of: ${template}`,
  keptIn: 'Kept in',
  change: 'Change…',
  changePlaceTitle: 'Choose where to keep new documents',
  placeIsADocument: 'That folder is itself a document. Choose the folder it is in, or another one.',
  back: 'Back',
  create: 'Create',
  creating: 'Creating…',
  titleNeeded: (field: string) => `Fill in “${field}”. The document’s folder is named after it.`,

  openTitle: 'Open a document',
  chooseFile: 'Choose a file…',
  chooseFileHint: 'a .tex file, or a .zip (from Overleaf)',
  chooseFileTitle: 'Choose a document',
  documentFiles: 'Documents (.tex, .zip)',
  everyFile: 'Every file',
  chooseFolder: 'Choose a folder…',
  chooseFolderTitle: 'Choose a document’s folder',
  findItTitle: 'Choose the folder it is in now',
  chooseFolderHint: 'if your document is a folder of files',
  getFromGit: 'Get one from GitHub or another Git address…',

  cloneCancelled: (folder: string) =>
    `What you cancelled had been fetched by then. It is in ${folder}: open it from there, or delete that folder.`,
  cloneNotOpened: (folder: string) =>
    `The document was fetched, but another one was being opened. It is in ${folder}: open it from there.`,
  cloneFailed: (why: string) => `The document could not be fetched: ${why}`,

  comingFromWord:
    'Coming from Word? Abstract can’t open Word files. Start a new document and paste your text into it.',
  newDocument: 'New document',

  looking: (name: string) => `Looking at ${name}…`,
  crowdedFolder: (folder: string) =>
    `${folder} holds many different things, so it can’t be opened as one document. Choose the folder of one document, or a .tex file.`,
  chooseAgain: 'Choose again…',
  noTex: (folder: string) =>
    `There is no .tex file in ${folder}, so there is nothing to make a PDF of yet.`,
  openAnyway: 'Open it anyway',
  /** What cannot be opened, and what to do about it. */
  refused: {
    word: 'That is a word processor’s file, and Abstract can’t open those. Start a new document, then copy your text there and paste it in.',
    pdf: 'That is a PDF, which can’t be edited. If you have the .tex file it was made from, choose that one.',
    otherArchive: 'That is an archive Abstract can’t unpack. Unpack it first (often: double-click it), then choose the folder.',
    otherFile: 'That is not a document Abstract can open. Choose a .tex file, or the folder it is in.',
    gone: 'It is not there any more. It may have been moved, renamed or deleted, or its drive is not connected.',
  },
  yourFolder: (name: string) => `your ${name} folder`,
  /** A `.tex` among other things. `folder` is where it is, `place` where a folder would be made. */
  strayQuestion: (file: string, folder: string, place: string, others: number) =>
    `${file} is in ${folder} with other things. Abstract keeps each document in a folder of its own. ` +
    `Make one in ${place} and copy ${file} there${
      others === 0 ? '' : others === 1 ? ', with the one file it uses' : `, with the ${others} files it uses`
    }? What is in ${folder} stays as it is.`,
  copyAndOpen: 'Copy and open',
  cancel: 'Cancel',
  /** Why a zip was not unpacked. Each is given the name it is about, though most are about none. */
  zipRefused: {
    notAZip: () => 'That zip is damaged, or is not really a zip. Download it again, or ask for it again.',
    locked: () => 'That zip needs a password, which Abstract can’t ask for. Unpack it first (often: double-click it), then choose the folder.',
    unsupported: () => 'That zip was made in a way Abstract can’t read. Unpack it first (often: double-click it), then choose the folder.',
    tooMany: () => 'That zip holds more than 20,000 files, too many for one document. If it is one, unpack it first, then choose the folder.',
    tooBig: () => 'That zip holds more than 2 GB, too much for one document. If it is one, unpack it first, then choose the folder.',
    unsafeName: (name: string) =>
      `That zip was not unpacked, because one thing in it can’t safely be written on this computer: “${name}”. Ask whoever made it for another.`,
    /** Without a name when they are the very same: then nothing says which. */
    sameName: (name: string) =>
      name
        ? `That zip was not unpacked, because two things in it have the same name but for capital letters: “${name}”. Ask whoever made it to rename one.`
        : 'That zip was not unpacked, because two things in it have the same name. Ask whoever made it to rename one.',
    noTex: () => 'There is no .tex file in that zip, so there is no document in it to open.',
    io: (name: string) =>
      `That zip could not be unpacked (${name}). The disk may be full, or the place for new documents may not be there.`,
  } satisfies Record<UnpackRefusal, (name: string) => string>,
  unpackingStopped: 'Unpacking was stopped.',
  /** A chosen file that is no part of the document the PDF is made from. */
  pdfShowsAnother: (main: string, chosen: string) =>
    `The PDF shows ${main}. ${chosen} is in the same folder, but is not part of that document.`,

  lookTitle: 'Text and colours',
  textSize: 'Text size',
  textSizeHint: 'Everything in the window gets larger. The PDF you save stays as it is.',
  percent: (size: number) => `${size}%`,
  coloursTitle: 'Colours',
  colours: {
    match: 'Match my computer',
    light: 'Light',
    dark: 'Dark',
    'contrast-light': 'High contrast, light',
    'contrast-dark': 'High contrast, dark',
  } satisfies Record<Colours, string>,

  opening: (name: string) => `Opening ${name}…`,
  stop: 'Stop',
} as const;
