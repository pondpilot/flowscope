/// <reference types="@testing-library/jest-dom" />
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { SchemaSearchControl } from '@/components/SchemaSearchControl';

afterEach(() => {
  cleanup();
});

describe('SchemaSearchControl', () => {
  it('renders as an icon-only button initially', () => {
    render(<SchemaSearchControl tableNames={['MARA', 'MANDT']} onSelectTable={vi.fn()} />);
    expect(screen.getByTestId('schema-search-toggle')).toBeInTheDocument();
    expect(screen.queryByTestId('schema-search-input')).not.toBeInTheDocument();
  });

  it('expands into an input on click', () => {
    render(<SchemaSearchControl tableNames={['MARA']} onSelectTable={vi.fn()} />);
    fireEvent.click(screen.getByTestId('schema-search-toggle'));
    expect(screen.getByTestId('schema-search-input')).toBeInTheDocument();
    expect(screen.queryByTestId('schema-search-toggle')).not.toBeInTheDocument();
  });

  it('selects a matching table on keystroke (case-insensitive prefix)', () => {
    const onSelectTable = vi.fn();
    render(
      <SchemaSearchControl tableNames={['MARA', 'BKPF', 'BSEG']} onSelectTable={onSelectTable} />
    );
    fireEvent.click(screen.getByTestId('schema-search-toggle'));
    const input = screen.getByTestId('schema-search-input');

    fireEvent.change(input, { target: { value: 'bk' } });
    expect(onSelectTable).toHaveBeenLastCalledWith('BKPF');

    fireEvent.change(input, { target: { value: 'MA' } });
    expect(onSelectTable).toHaveBeenLastCalledWith('MARA');
  });

  it('passes undefined to onSelectTable when no table matches', () => {
    const onSelectTable = vi.fn();
    render(<SchemaSearchControl tableNames={['MARA']} onSelectTable={onSelectTable} />);
    fireEvent.click(screen.getByTestId('schema-search-toggle'));
    const input = screen.getByTestId('schema-search-input');

    fireEvent.change(input, { target: { value: 'zzz' } });
    expect(onSelectTable).toHaveBeenLastCalledWith(undefined);
  });

  it('clears selection when the input is emptied', () => {
    const onSelectTable = vi.fn();
    render(<SchemaSearchControl tableNames={['MARA']} onSelectTable={onSelectTable} />);
    fireEvent.click(screen.getByTestId('schema-search-toggle'));
    const input = screen.getByTestId('schema-search-input');

    fireEvent.change(input, { target: { value: 'MA' } });
    expect(onSelectTable).toHaveBeenLastCalledWith('MARA');

    fireEvent.change(input, { target: { value: '' } });
    expect(onSelectTable).toHaveBeenLastCalledWith(undefined);
  });

  it('collapses back to icon when close button is clicked', () => {
    const onSelectTable = vi.fn();
    render(<SchemaSearchControl tableNames={['MARA']} onSelectTable={onSelectTable} />);
    fireEvent.click(screen.getByTestId('schema-search-toggle'));
    fireEvent.change(screen.getByTestId('schema-search-input'), {
      target: { value: 'ma' },
    });

    fireEvent.click(screen.getByTestId('schema-search-close'));
    expect(screen.queryByTestId('schema-search-input')).not.toBeInTheDocument();
    expect(screen.getByTestId('schema-search-toggle')).toBeInTheDocument();
    // Selection cleared on collapse
    expect(onSelectTable).toHaveBeenLastCalledWith(undefined);
  });

  it('collapses on blur when the input is empty', () => {
    render(<SchemaSearchControl tableNames={['MARA']} onSelectTable={vi.fn()} />);
    fireEvent.click(screen.getByTestId('schema-search-toggle'));
    const input = screen.getByTestId('schema-search-input');

    fireEvent.blur(input);
    expect(screen.queryByTestId('schema-search-input')).not.toBeInTheDocument();
    expect(screen.getByTestId('schema-search-toggle')).toBeInTheDocument();
  });

  it('does not collapse on blur when there is text in the field', () => {
    render(<SchemaSearchControl tableNames={['MARA']} onSelectTable={vi.fn()} />);
    fireEvent.click(screen.getByTestId('schema-search-toggle'));
    const input = screen.getByTestId('schema-search-input');

    fireEvent.change(input, { target: { value: 'ma' } });
    fireEvent.blur(input);
    expect(screen.getByTestId('schema-search-input')).toBeInTheDocument();
  });

  it('collapses and clears selection on Escape', () => {
    const onSelectTable = vi.fn();
    render(<SchemaSearchControl tableNames={['MARA']} onSelectTable={onSelectTable} />);
    fireEvent.click(screen.getByTestId('schema-search-toggle'));
    const input = screen.getByTestId('schema-search-input');

    fireEvent.change(input, { target: { value: 'ma' } });
    fireEvent.keyDown(input, { key: 'Escape' });

    expect(screen.queryByTestId('schema-search-input')).not.toBeInTheDocument();
    expect(screen.getByTestId('schema-search-toggle')).toBeInTheDocument();
    expect(onSelectTable).toHaveBeenLastCalledWith(undefined);
  });

  it('does not re-clear selection on parent re-render after collapse', () => {
    // Regression: hasInteractedRef leaked across search sessions. After the
    // user typed once and then closed the control, a subsequent parent
    // re-render would fire the selection effect with empty matches and an
    // already-true ref, calling onSelectTable(undefined) again — which would
    // clobber any selection set externally between collapse and re-render.
    const onSelectTable = vi.fn();
    const { rerender } = render(
      <SchemaSearchControl tableNames={['MARA', 'BKPF']} onSelectTable={onSelectTable} />
    );
    fireEvent.click(screen.getByTestId('schema-search-toggle'));
    fireEvent.change(screen.getByTestId('schema-search-input'), { target: { value: 'MA' } });
    fireEvent.click(screen.getByTestId('schema-search-close'));

    expect(onSelectTable).toHaveBeenLastCalledWith(undefined);
    const callsAfterCollapse = onSelectTable.mock.calls.length;

    rerender(
      <SchemaSearchControl tableNames={['MARA', 'BKPF', 'NEW']} onSelectTable={onSelectTable} />
    );

    expect(onSelectTable.mock.calls.length).toBe(callsAfterCollapse);
  });

  it('clamps the active match when the match list shrinks', () => {
    const onSelectTable = vi.fn();
    const { rerender } = render(
      <SchemaSearchControl tableNames={['MARA', 'MANDT', 'MATDOC']} onSelectTable={onSelectTable} />
    );
    fireEvent.click(screen.getByTestId('schema-search-toggle'));
    const input = screen.getByTestId('schema-search-input');

    fireEvent.change(input, { target: { value: 'ma' } });
    fireEvent.click(screen.getByTestId('schema-search-next'));
    fireEvent.click(screen.getByTestId('schema-search-next'));
    expect(screen.getByText('3/3')).toBeInTheDocument();

    rerender(<SchemaSearchControl tableNames={['MARA']} onSelectTable={onSelectTable} />);

    expect(screen.getByText('1/1')).toBeInTheDocument();
    expect(onSelectTable).toHaveBeenLastCalledWith('MARA');
  });

  it('does not reselect the same match when parent props get new array identities', () => {
    const onSelectTable = vi.fn();
    const { rerender } = render(
      <SchemaSearchControl tableNames={['MARA', 'BKPF']} onSelectTable={onSelectTable} />
    );
    fireEvent.click(screen.getByTestId('schema-search-toggle'));
    fireEvent.change(screen.getByTestId('schema-search-input'), { target: { value: 'MA' } });

    expect(onSelectTable).toHaveBeenCalledTimes(1);
    expect(onSelectTable).toHaveBeenLastCalledWith('MARA');

    rerender(<SchemaSearchControl tableNames={['MARA', 'BKPF']} onSelectTable={onSelectTable} />);

    expect(onSelectTable).toHaveBeenCalledTimes(1);
  });
});
