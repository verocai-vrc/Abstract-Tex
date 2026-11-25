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

  pages: { home: 'Home', new: 'New', open: 'Open', look: 'Text and colours' },

  /** Where there is room for one pane (S17.13). */
  panesLabel: 'What to show',
  panes: { files: 'Files', write: 'Write', pdf: 'PDF' },

  /** Over an open document: the way back to it. */
  backTo: (title: string) => `Back to ${title}`,
  openNow: 'Open now',

  savePdf: 'Save PDF…',
  savePdfTitle: 'Save the PDF',
  savingPdf: 'Getting the newest PDF ready to save…',
  savedPdf: (name: string, place: string) => `Saved ${name} in ${place}.`,
  savedOlderPdf: (name: string, place: string) =>
    `Saved ${name} in ${place}. It is an earlier version: your newest text could not be made into a PDF.`,
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
  back: 'Back',
  create: 'Create',
  creating: 'Creating…',
  titleNeeded: 'Give the document a title. Its folder is named after it.',

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
    sameName: (name: string) =>
      `That zip was not unpacked, because two things in it have the same name but for capital letters: “${name}”. Ask whoever made it to rename one.`,
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
