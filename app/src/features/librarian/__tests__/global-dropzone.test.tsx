/// <reference types="@testing-library/jest-dom" />
import { act, cleanup, render } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

// ---------- Mocks ----------

const mockImportFiles = vi.fn();

vi.mock('@/lib/project-store', () => ({
  useProject: () => ({
    importFiles: mockImportFiles,
    currentProject: { id: 'test-project' },
    isReadOnly: false,
  }),
}));

vi.mock('@/lib/constants', () => ({
  ACCEPTED_FILE_TYPES_ARRAY: ['.sql'],
  FILE_LIMITS: { MAX_SIZE: 10 * 1024 * 1024, MAX_COUNT: 1000 },
}));

import { GlobalDropZone } from '@/components/GlobalDropZone';

beforeEach(() => {
  vi.clearAllMocks();
});

afterEach(() => {
  cleanup();
});

describe('GlobalDropZone', () => {
  it('ignores drops inside a librarian dropzone element', async () => {
    // Render GlobalDropZone and a librarian dropzone together
    const { container } = render(
      <div>
        <GlobalDropZone />
        <div data-librarian-dropzone data-testid="librarian-zone">
          <span data-testid="inner-target">Drop here</span>
        </div>
      </div>
    );

    // Simulate dragenter to activate the overlay
    const dragEnterEvent = new Event('dragenter', { bubbles: true });
    Object.defineProperty(dragEnterEvent, 'dataTransfer', {
      value: { types: ['Files'], dropEffect: 'copy' },
    });
    Object.defineProperty(dragEnterEvent, 'preventDefault', { value: vi.fn() });
    Object.defineProperty(dragEnterEvent, 'stopPropagation', { value: vi.fn() });
    window.dispatchEvent(dragEnterEvent);

    // Create a drop event whose target is inside the librarian dropzone
    const innerTarget = container.querySelector('[data-testid="inner-target"]')!;
    const dropEvent = new Event('drop', { bubbles: true });
    Object.defineProperty(dropEvent, 'target', { value: innerTarget });
    Object.defineProperty(dropEvent, 'dataTransfer', {
      value: {
        items: [{ kind: 'file', type: 'application/pdf' }],
        files: [new File(['pdf'], 'test.pdf', { type: 'application/pdf' })],
      },
    });
    Object.defineProperty(dropEvent, 'preventDefault', { value: vi.fn() });
    Object.defineProperty(dropEvent, 'stopPropagation', { value: vi.fn() });

    window.dispatchEvent(dropEvent);

    // importFiles should NOT have been called because drop was inside librarian zone
    expect(mockImportFiles).not.toHaveBeenCalled();
  });

  it('hides overlay when dragging over librarian dropzone and reappears when leaving', () => {
    const { container } = render(
      <div>
        <GlobalDropZone />
        <div data-librarian-dropzone data-testid="librarian-zone">
          <span data-testid="inner-target">Drop here</span>
        </div>
      </div>
    );

    // Simulate dragenter on window to activate overlay
    const dragEnterEvent = new Event('dragenter', { bubbles: true });
    Object.defineProperty(dragEnterEvent, 'dataTransfer', {
      value: { types: ['Files'], dropEffect: 'copy' },
    });
    Object.defineProperty(dragEnterEvent, 'preventDefault', { value: vi.fn() });
    Object.defineProperty(dragEnterEvent, 'stopPropagation', { value: vi.fn() });
    act(() => {
      window.dispatchEvent(dragEnterEvent);
    });

    // Overlay should be visible
    let overlay = container.querySelector('[aria-label="File drop zone"]');
    expect(overlay).toBeInTheDocument();

    // Simulate dragover with target inside librarian zone — overlay should hide
    const innerTarget = container.querySelector('[data-testid="inner-target"]')!;
    const dragOverEvent = new Event('dragover', { bubbles: true });
    Object.defineProperty(dragOverEvent, 'target', { value: innerTarget });
    Object.defineProperty(dragOverEvent, 'dataTransfer', {
      value: { types: ['Files'], dropEffect: 'copy' },
    });
    Object.defineProperty(dragOverEvent, 'preventDefault', { value: vi.fn() });
    Object.defineProperty(dragOverEvent, 'stopPropagation', { value: vi.fn() });
    act(() => {
      window.dispatchEvent(dragOverEvent);
    });

    // Overlay should be gone
    overlay = container.querySelector('[aria-label="File drop zone"]');
    expect(overlay).not.toBeInTheDocument();

    // Simulate dragenter again outside librarian area — overlay should reappear
    const reEnterEvent = new Event('dragenter', { bubbles: true });
    Object.defineProperty(reEnterEvent, 'dataTransfer', {
      value: { types: ['Files'], dropEffect: 'copy' },
    });
    Object.defineProperty(reEnterEvent, 'preventDefault', { value: vi.fn() });
    Object.defineProperty(reEnterEvent, 'stopPropagation', { value: vi.fn() });
    act(() => {
      window.dispatchEvent(reEnterEvent);
    });

    overlay = container.querySelector('[aria-label="File drop zone"]');
    expect(overlay).toBeInTheDocument();
  });

  it('processes drops outside librarian dropzone normally', async () => {
    render(
      <div>
        <GlobalDropZone />
        <div data-testid="normal-area">
          <span data-testid="normal-target">Regular area</span>
        </div>
      </div>
    );

    // Simulate dragenter
    const dragEnterEvent = new Event('dragenter', { bubbles: true });
    Object.defineProperty(dragEnterEvent, 'dataTransfer', {
      value: { types: ['Files'], dropEffect: 'copy' },
    });
    Object.defineProperty(dragEnterEvent, 'preventDefault', { value: vi.fn() });
    Object.defineProperty(dragEnterEvent, 'stopPropagation', { value: vi.fn() });
    window.dispatchEvent(dragEnterEvent);

    // Create a drop event with a regular target (no librarian dropzone ancestor)
    const regularTarget = document.createElement('div');
    document.body.appendChild(regularTarget);

    const sqlFile = new File(['SELECT 1'], 'query.sql', { type: 'text/plain' });
    Object.defineProperty(sqlFile, 'name', { value: 'query.sql' });

    const dropEvent = new Event('drop', { bubbles: true });
    Object.defineProperty(dropEvent, 'target', { value: regularTarget });
    Object.defineProperty(dropEvent, 'dataTransfer', {
      value: {
        items: [{ kind: 'file', type: 'text/plain' }],
        files: [sqlFile],
      },
    });
    Object.defineProperty(dropEvent, 'preventDefault', { value: vi.fn() });
    Object.defineProperty(dropEvent, 'stopPropagation', { value: vi.fn() });

    window.dispatchEvent(dropEvent);

    // Wait for async processing
    await vi.waitFor(() => {
      expect(mockImportFiles).toHaveBeenCalled();
    });

    document.body.removeChild(regularTarget);
  });
});
