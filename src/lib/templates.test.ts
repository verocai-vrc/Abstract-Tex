import { describe, expect, it } from 'vitest';
import { categoriesOf, categoryLabel, filterTemplates, neighbour } from './templates.svelte';
import type { TemplateInfo } from './ipc';

function template(id: string, category: string, description = ''): TemplateInfo {
  return { id, name: id[0]!.toUpperCase() + id.slice(1), category, description, previewUrl: '', fields: [] };
}

const catalog = [
  template('blank', 'other', 'A title and an empty page'),
  template('essay', 'essay', 'A short essay with sections'),
  template('report', 'report', 'Chapters and a contents page'),
  template('paper', 'paper', 'An abstract, sections and a bibliography'),
  template('cv', 'cv', 'One page, no photo'),
];

describe('categoriesOf', () => {
  it('lists each category once, in the order the catalog first uses it', () => {
    expect(categoriesOf([...catalog, template('essay-two', 'essay')])).toEqual(['other', 'essay', 'report', 'paper', 'cv']);
  });
});

describe('categoryLabel', () => {
  it('capitalises, and knows that a CV is a CV', () => {
    expect(categoryLabel('essay')).toBe('Essay');
    expect(categoryLabel('cv')).toBe('CV');
    expect(categoryLabel('all')).toBe('All');
    expect(categoryLabel('other')).toBe('Other');
  });
});

describe('filterTemplates', () => {
  const ids = (list: TemplateInfo[]) => list.map((t) => t.id);

  it('keeps everything for "all" and an empty query, in catalog order', () => {
    expect(ids(filterTemplates(catalog, 'all', '  '))).toEqual(['blank', 'essay', 'report', 'paper', 'cv']);
  });

  it('filters by category', () => {
    expect(ids(filterTemplates(catalog, 'cv', ''))).toEqual(['cv']);
  });

  it('matches every word anywhere in the name, category or description, ignoring case', () => {
    expect(ids(filterTemplates(catalog, 'all', 'SECTIONS'))).toEqual(['essay', 'paper']);
    expect(ids(filterTemplates(catalog, 'all', 'sections bibliography'))).toEqual(['paper']);
    expect(ids(filterTemplates(catalog, 'all', 'cv'))).toEqual(['cv']);
  });

  it('combines the category with the query', () => {
    expect(ids(filterTemplates(catalog, 'essay', 'bibliography'))).toEqual([]);
  });
});

describe('neighbour', () => {
  it('steps along the shown cards and stops at the ends', () => {
    expect(neighbour(catalog, 'essay', 1)).toBe('report');
    expect(neighbour(catalog, 'essay', -1)).toBe('blank');
    expect(neighbour(catalog, 'blank', -1)).toBe('blank');
    expect(neighbour(catalog, 'cv', 1)).toBe('cv');
  });

  it('starts at the first card when nothing, or something filtered away, is highlighted', () => {
    expect(neighbour(catalog, null, 1)).toBe('blank');
    expect(neighbour(catalog.slice(2), 'essay', 1)).toBe('report');
    expect(neighbour([], 'essay', 1)).toBeNull();
  });
});
