import { useCallback, useEffect, useRef, useState } from 'react';
import type { MatrixViewControlledState } from './types';

export function useImmediateControlledMatrixState<K extends keyof MatrixViewControlledState>(
  key: K,
  controlledState: Partial<MatrixViewControlledState> | undefined,
  onStateChange: ((state: Partial<MatrixViewControlledState>) => void) | undefined,
  defaultValue: MatrixViewControlledState[K]
): [
  MatrixViewControlledState[K],
  (value: React.SetStateAction<MatrixViewControlledState[K]>) => void,
] {
  const controlledValue = controlledState?.[key] as MatrixViewControlledState[K] | undefined;
  const [value, setValue] = useState<MatrixViewControlledState[K]>(() =>
    controlledValue !== undefined ? controlledValue : defaultValue
  );
  const prevControlledValueRef = useRef(controlledValue);

  useEffect(() => {
    if (controlledValue !== prevControlledValueRef.current) {
      prevControlledValueRef.current = controlledValue;
      if (controlledValue !== undefined) {
        setValue(controlledValue);
      }
    }
  }, [controlledValue]);

  const setValueImmediate = useCallback(
    (valueOrUpdater: React.SetStateAction<MatrixViewControlledState[K]>) => {
      setValue((prev) => {
        const nextValue =
          typeof valueOrUpdater === 'function'
            ? (
                valueOrUpdater as (
                  prevState: MatrixViewControlledState[K]
                ) => MatrixViewControlledState[K]
              )(prev)
            : valueOrUpdater;

        onStateChange?.({ [key]: nextValue });
        return nextValue;
      });
    },
    [key, onStateChange]
  );

  return [value, setValueImmediate];
}
