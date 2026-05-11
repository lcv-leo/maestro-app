import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { ErrorBoundary } from './ErrorBoundary';

vi.mock('../diagnostics', () => ({
  logEvent: vi.fn(),
}));

function BrokenChild() {
  throw new Error('render failed');
  return null;
}

describe('ErrorBoundary', () => {
  it('renders a recoverable alert when a child throws during render', () => {
    const consoleSpy = vi.spyOn(console, 'error').mockImplementation(() => undefined);

    render(
      <ErrorBoundary>
        <BrokenChild />
      </ErrorBoundary>,
    );

    expect(screen.getByRole('alert')).toHaveTextContent('Algo deu errado no Maestro.');
    expect(screen.getByRole('button', { name: 'Recarregar' })).toBeInTheDocument();

    consoleSpy.mockRestore();
  });
});
