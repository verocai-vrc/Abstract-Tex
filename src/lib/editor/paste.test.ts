import { describe, expect, it, vi } from 'vitest';
import { identify, pasteCiteHandler } from './paste';

describe('identify', () => {
  it('recognises a doi.org URL, normalized to the bare DOI', () => {
    expect(identify('https://doi.org/10.1109/tcbb.2019.000001')).toEqual({ kind: 'doi', normalized: '10.1109/tcbb.2019.000001' });
  });

  it('recognises a bare DOI', () => {
    expect(identify('10.1371/journal.pcbi.1000387')).toEqual({ kind: 'doi', normalized: '10.1371/journal.pcbi.1000387' });
  });

  it('recognises a new-style arXiv id with the arXiv: scheme', () => {
    expect(identify('arXiv:1706.03762')).toEqual({ kind: 'arxiv', normalized: '1706.03762' });
  });

  it('keeps a version suffix on a new-style arXiv id', () => {
    expect(identify('1706.03762v7')).toEqual({ kind: 'arxiv', normalized: '1706.03762v7' });
  });

  it('recognises an old-style arXiv id', () => {
    expect(identify('hep-th/9901001')).toEqual({ kind: 'arxiv', normalized: 'hep-th/9901001' });
  });

  it('recognises an arXiv abs URL', () => {
    expect(identify('https://arxiv.org/abs/2101.00001')).toEqual({ kind: 'arxiv', normalized: '2101.00001' });
  });

  it('recognises a 13-digit ISBN with hyphens', () => {
    expect(identify('978-0-262-03384-8')).toEqual({ kind: 'isbn', normalized: '9780262033848' });
  });

  it('recognises a 10-digit ISBN with an X check digit', () => {
    expect(identify('0-306-40615-x')).toEqual({ kind: 'isbn', normalized: '030640615X' });
  });

  it('identifies a plain sentence as nothing', () => {
    expect(identify('A Probabilistic Model of DNA Folding')).toBeNull();
  });

  it('identifies an empty or whitespace paste as nothing', () => {
    expect(identify('')).toBeNull();
    expect(identify('   ')).toBeNull();
  });

  it('identifies a run of the wrong digit count as nothing', () => {
    expect(identify('12345')).toBeNull();
    expect(identify('123456789012345')).toBeNull();
  });
});

describe('pasteCiteHandler', () => {
  function clipboardEvent(text: string): ClipboardEvent {
    return {
      clipboardData: { getData: (type: string) => (type === 'text/plain' ? text : '') },
      preventDefault: vi.fn(),
    } as unknown as ClipboardEvent;
  }

  function viewWithSelection(from: number, to: number) {
    return { state: { selection: { main: { from, to } } } } as unknown as never;
  }

  it('intercepts a recognised paste and calls the requester with the captured selection', () => {
    const request = vi.fn().mockResolvedValue(undefined);
    const handler = pasteCiteHandler(request);
    const event = clipboardEvent('10.1109/tcbb.2019.000001');
    const view = viewWithSelection(5, 9);
    const handled = handler(event, view);
    expect(handled).toBe(true);
    expect(event.preventDefault).toHaveBeenCalled();
    expect(request).toHaveBeenCalledWith('10.1109/tcbb.2019.000001', view, 5, 9);
  });

  it('leaves an ordinary paste alone', () => {
    const request = vi.fn();
    const handler = pasteCiteHandler(request);
    const event = clipboardEvent('some prose the author is pasting in');
    const handled = handler(event, viewWithSelection(0, 0));
    expect(handled).toBe(false);
    expect(event.preventDefault).not.toHaveBeenCalled();
    expect(request).not.toHaveBeenCalled();
  });
});
