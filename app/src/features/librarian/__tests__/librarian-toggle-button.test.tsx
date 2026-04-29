/// <reference types="@testing-library/jest-dom" />
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import { LibrarianToggleButton } from '@/components/LibrarianToggleButton';
import { useViewStateStore } from '@/lib/view-state-store';

beforeEach(() => {
  useViewStateStore.setState({ librarianOpen: false });
});

afterEach(() => {
  cleanup();
});

describe('LibrarianToggleButton', () => {
  it('renders the Librarian label and Polly icon', () => {
    render(<LibrarianToggleButton />);
    const button = screen.getByTestId('librarian-toggle-button');
    expect(button).toBeInTheDocument();
    expect(button).toHaveTextContent('Librarian');
    const icon = button.querySelector('img');
    expect(icon).not.toBeNull();
    expect(icon?.getAttribute('src')).toBe('/polly-icon.svg');
  });

  it('reflects the closed state with aria-pressed=false', () => {
    render(<LibrarianToggleButton />);
    const button = screen.getByTestId('librarian-toggle-button');
    expect(button).toHaveAttribute('aria-pressed', 'false');
  });

  it('toggles the store state when clicked', () => {
    render(<LibrarianToggleButton />);
    const button = screen.getByTestId('librarian-toggle-button');

    fireEvent.click(button);
    expect(useViewStateStore.getState().librarianOpen).toBe(true);

    fireEvent.click(button);
    expect(useViewStateStore.getState().librarianOpen).toBe(false);
  });

  it('reflects the open state with aria-pressed=true when the panel is open', () => {
    useViewStateStore.setState({ librarianOpen: true });
    render(<LibrarianToggleButton />);
    const button = screen.getByTestId('librarian-toggle-button');
    expect(button).toHaveAttribute('aria-pressed', 'true');
  });
});
