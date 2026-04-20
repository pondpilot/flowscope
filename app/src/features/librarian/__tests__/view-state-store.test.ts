import { describe, it, expect, beforeEach } from 'vitest';
import { useViewStateStore } from '@/lib/view-state-store';

describe('view-state-store - librarian', () => {
  beforeEach(() => {
    // Reset the store state between tests
    useViewStateStore.setState({ librarianOpen: false });
  });

  it('defaults librarianOpen to false', () => {
    expect(useViewStateStore.getState().librarianOpen).toBe(false);
  });

  it('toggleLibrarian toggles the value', () => {
    const { toggleLibrarian } = useViewStateStore.getState();

    toggleLibrarian();
    expect(useViewStateStore.getState().librarianOpen).toBe(true);

    toggleLibrarian();
    expect(useViewStateStore.getState().librarianOpen).toBe(false);
  });

  it('setLibrarianOpen sets the value directly', () => {
    const { setLibrarianOpen } = useViewStateStore.getState();

    setLibrarianOpen(true);
    expect(useViewStateStore.getState().librarianOpen).toBe(true);

    setLibrarianOpen(false);
    expect(useViewStateStore.getState().librarianOpen).toBe(false);
  });

  it('librarianOpen is included in persisted state', () => {
    useViewStateStore.getState().setLibrarianOpen(true);

    const persisted = localStorage.getItem('flowscope-view-states');
    expect(persisted).not.toBeNull();
    const parsed = JSON.parse(persisted as string);
    expect(parsed.state).toHaveProperty('librarianOpen', true);

    useViewStateStore.getState().setLibrarianOpen(false);
    const persistedAfter = JSON.parse(localStorage.getItem('flowscope-view-states') as string);
    expect(persistedAfter.state).toHaveProperty('librarianOpen', false);
  });

  it('toggleLibrarian does not affect other store state', () => {
    const { setActiveTab, toggleLibrarian } = useViewStateStore.getState();
    setActiveTab('test-project', 'hierarchy');

    toggleLibrarian();

    expect(useViewStateStore.getState().librarianOpen).toBe(true);
    expect(useViewStateStore.getState().getActiveTab('test-project')).toBe('hierarchy');
  });
});
