import { describe, expect, it } from 'vitest';
import { LspClient, languageIdFor, pathToUri, uriToPath, type LspTransport } from './lsp';

/** Records every call instead of talking to a server. */
function recorder() {
  const sent: Array<{ method: string; params: any }> = [];
  const transport: LspTransport = {
    notify: (method, params) => {
      sent.push({ method, params: params as any });
    },
    request: async (method, params) => {
      sent.push({ method, params: params as any });
      return null as any;
    },
  };
  return { sent, transport, only: (method: string) => sent.filter((c) => c.method === method) };
}

describe('uri conversion', () => {
  it('gives a Windows path the third slash and forward slashes', () => {
    expect(pathToUri('C:\\Users\\a\\main.tex')).toBe('file:///C:/Users/a/main.tex');
  });

  it('leaves a unix path with exactly two slashes after the scheme', () => {
    expect(pathToUri('/home/a/main.tex')).toBe('file:///home/a/main.tex');
  });

  it('round-trips both shapes', () => {
    for (const path of ['/home/a/main.tex', 'C:\\Users\\a\\main.tex']) {
      const back = uriToPath(pathToUri(path));
      expect(back).toBe(path.replace(/\\/g, '/'));
    }
  });

  it('percent-encodes a space, which an unencoded uri would make TexLab reject', () => {
    expect(pathToUri('C:\\LaTeX Editor\\main.tex')).toBe('file:///C:/LaTeX%20Editor/main.tex');
  });

  it('encodes the characters that would otherwise change how the uri parses', () => {
    expect(pathToUri('/a/b#c?d.tex')).toBe('file:///a/b%23c%3Fd.tex');
  });

  it('round-trips a path with spaces', () => {
    expect(uriToPath(pathToUri('/a/my paper/main.tex'))).toBe('/a/my paper/main.tex');
  });

  it('decodes the percent-encoding a server may send back', () => {
    expect(uriToPath('file:///home/a/my%20paper.tex')).toBe('/home/a/my paper.tex');
  });
});

describe('languageIdFor', () => {
  it('knows the two kinds of file the server cares about', () => {
    expect(languageIdFor('/p/main.tex')).toBe('latex');
    expect(languageIdFor('/p/refs.bib')).toBe('bibtex');
  });

  it('returns null for anything else, so we never open a PDF as source', () => {
    expect(languageIdFor('/p/figure.pdf')).toBeNull();
    expect(languageIdFor('/p/notes.md')).toBeNull();
  });
});

describe('LspClient document sync', () => {
  it('opens a document once, with version 1', async () => {
    const { transport, only } = recorder();
    const client = new LspClient(transport);
    await client.didOpen('/p/main.tex', 'hello');

    const opens = only('textDocument/didOpen');
    expect(opens).toHaveLength(1);
    expect(opens[0]?.params.textDocument).toMatchObject({
      uri: 'file:///p/main.tex',
      languageId: 'latex',
      version: 1,
      text: 'hello',
    });
  });

  it('does not send a second didOpen for a file already open', async () => {
    const { transport, only } = recorder();
    const client = new LspClient(transport);
    await client.didOpen('/p/main.tex', 'hello');
    await client.didOpen('/p/main.tex', 'hello again');
    expect(only('textDocument/didOpen')).toHaveLength(1);
  });

  it('never opens a file the server has no language for', async () => {
    const { transport, sent } = recorder();
    const client = new LspClient(transport);
    await client.didOpen('/p/figure.pdf', 'binary');
    expect(sent).toHaveLength(0);
    expect(client.openUris()).toEqual([]);
  });

  it('increments the version on every real change', async () => {
    const { transport } = recorder();
    const client = new LspClient(transport);
    await client.didOpen('/p/main.tex', 'a');
    await client.didChange('/p/main.tex', 'ab');
    await client.didChange('/p/main.tex', 'abc');
    expect(client.versionOf('file:///p/main.tex')).toBe(3);
  });

  it('does not bump the version when the text is unchanged', async () => {
    const { transport, only } = recorder();
    const client = new LspClient(transport);
    await client.didOpen('/p/main.tex', 'a');
    await client.didChange('/p/main.tex', 'a');
    expect(only('textDocument/didChange')).toHaveLength(0);
    expect(client.versionOf('file:///p/main.tex')).toBe(1);
  });

  it('ignores a change to a file that was never opened', async () => {
    const { transport, sent } = recorder();
    const client = new LspClient(transport);
    await client.didChange('/p/ghost.tex', 'text');
    expect(sent).toHaveLength(0);
  });

  it('closing forgets the document and a later change is silent', async () => {
    const { transport, only } = recorder();
    const client = new LspClient(transport);
    await client.didOpen('/p/main.tex', 'a');
    await client.didClose('/p/main.tex');
    await client.didChange('/p/main.tex', 'b');

    expect(only('textDocument/didClose')).toHaveLength(1);
    expect(only('textDocument/didChange')).toHaveLength(0);
    expect(client.openUris()).toEqual([]);
  });

  /** The test this module exists for. A restarted server has never heard of these files, so
   * continuing from version 4 would leave it permanently out of step with the editor. */
  it('resync re-opens every document at version 1 with the latest text', async () => {
    const { transport, only } = recorder();
    const client = new LspClient(transport);
    await client.didOpen('/p/main.tex', 'one');
    await client.didOpen('/p/refs.bib', '@book{}');
    await client.didChange('/p/main.tex', 'one two');
    await client.didChange('/p/main.tex', 'one two three');
    expect(client.versionOf('file:///p/main.tex')).toBe(3);

    await client.resync();

    const reopened = only('textDocument/didOpen').slice(2); // the first two were the originals
    expect(reopened).toHaveLength(2);
    expect(reopened[0]?.params.textDocument).toMatchObject({
      uri: 'file:///p/main.tex',
      version: 1,
      text: 'one two three', // the latest text, not the text it was first opened with
    });
    expect(reopened[1]?.params.textDocument).toMatchObject({ uri: 'file:///p/refs.bib', version: 1 });
    expect(client.versionOf('file:///p/main.tex')).toBe(1);
  });

  it('a change after a resync counts up from the new baseline', async () => {
    const { transport } = recorder();
    const client = new LspClient(transport);
    await client.didOpen('/p/main.tex', 'a');
    await client.didChange('/p/main.tex', 'ab');
    await client.resync();
    await client.didChange('/p/main.tex', 'abc');
    expect(client.versionOf('file:///p/main.tex')).toBe(2);
  });

  it('reset forgets everything without telling the server', async () => {
    const { transport, sent } = recorder();
    const client = new LspClient(transport);
    await client.didOpen('/p/main.tex', 'a');
    const before = sent.length;
    client.reset();
    expect(client.openUris()).toEqual([]);
    expect(sent).toHaveLength(before);
  });

  /**
   * The race the reviewer found: the save debounce and a completion request's just-in-time
   * flush (`controller.svelte.ts`'s `lspCompletion`) can both call `didChange` for the same
   * file. Each bump of `document.version` happens synchronously, so the numbers themselves are
   * never wrong — but before this fix, the two `notify` calls could still be in flight at once,
   * and whichever one's underlying IPC round trip resolved first is the one TexLab would see
   * first. A transport whose *second* call resolves before its *first* is exactly that ordering
   * — the version-6 change would have reached the wire before version 5.
   */
  it('never lets a later didChange notify before an earlier one for the same file', async () => {
    const arrived: number[] = [];
    let releaseFirst: () => void = () => {};
    const firstIsBlocked = new Promise<void>((resolve) => {
      releaseFirst = resolve;
    });
    let callCount = 0;
    const transport: LspTransport = {
      notify: async (method, params: any) => {
        callCount++;
        if (method === 'textDocument/didChange' && callCount === 1) {
          // Hold the first call open until the test explicitly releases it, simulating a slow
          // IPC round trip that a second, faster call could otherwise overtake.
          await firstIsBlocked;
        }
        if (method === 'textDocument/didChange') arrived.push(params.textDocument.version);
      },
      request: async () => null as never,
    };
    const client = new LspClient(transport);
    await client.didOpen('/p/main.tex', 'a');

    const first = client.didChange('/p/main.tex', 'ab'); // version 2, held open
    const second = client.didChange('/p/main.tex', 'abc'); // version 3, queued behind it

    releaseFirst();
    await Promise.all([first, second]);

    expect(arrived).toEqual([2, 3]); // never [3, 2], no matter which IPC call would return first
  });
});

describe('LspClient queries', () => {
  it('asks at the position it was given, addressing the file by uri', async () => {
    const { transport, only } = recorder();
    const client = new LspClient(transport);
    await client.completion('C:\\p\\main.tex', 4, 12);

    const [call] = only('textDocument/completion');
    expect(call?.params).toEqual({
      textDocument: { uri: 'file:///C:/p/main.tex' },
      position: { line: 4, character: 12 },
    });
  });

  it('hover, definition and symbols all address the same uri', async () => {
    const { transport, sent } = recorder();
    const client = new LspClient(transport);
    await client.hover('/p/main.tex', 0, 0);
    await client.definition('/p/main.tex', 1, 1);
    await client.documentSymbols('/p/main.tex');
    expect(sent.map((c) => c.method)).toEqual([
      'textDocument/hover',
      'textDocument/definition',
      'textDocument/documentSymbol',
    ]);
    for (const call of sent) {
      expect(call.params.textDocument.uri).toBe('file:///p/main.tex');
    }
  });
});
