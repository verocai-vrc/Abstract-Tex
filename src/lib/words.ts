// Every sentence of the start screen (S17.4), in one place and in English, so that a translation has
// one file to translate. Plain strings, and functions where something is filled in. No component of
// the start screen has words of its own (`src/components/start.test.ts` sees to it).
//
// Who reads these: people who have only used Word. The thing they work on is a *document*; the folder
// it is kept in is small print. Nothing here says project, repository or compile.

export const words = {
  appName: 'Abstract',
  /** The name of the list down the left, for a screen reader. */
  startNavigation: 'Start',

  pages: { home: 'Home', new: 'New', open: 'Open' },

  /** Over an open document: the way back to it. */
  backTo: (title: string) => `Back to ${title}`,
  openNow: 'Open now',

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

  openTitle: 'Open a document',
  chooseFolder: 'Choose a folder…',
  chooseFolderHint: 'if your document is a folder of files',
  getFromGit: 'Get one from GitHub or another Git address…',

  opening: (name: string) => `Opening ${name}…`,
  stop: 'Stop',
} as const;
