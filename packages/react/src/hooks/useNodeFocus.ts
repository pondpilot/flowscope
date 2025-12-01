import { useEffect, useRef } from 'react';
import { useReactFlow } from '@xyflow/react';

/**
 * Delay before focusing on a selected node after navigation.
 * Required because ReactFlow needs time to render nodes before we can
 * query their positions for fitView. Without this delay, getNode may
 * return undefined or stale position data.
 */
const NODE_FOCUS_DELAY_MS = 100;

interface UseNodeFocusOptions {
  /** Node ID to focus on */
  focusNodeId?: string;
  /** Callback when focus has been applied */
  onFocusApplied?: () => void;
  /** Duration of the fitView animation in ms */
  duration?: number;
  /** Padding around the focused node */
  padding?: number;
}

/**
 * Hook to handle programmatic node focusing in ReactFlow.
 * Centers the viewport on the specified node with animation.
 *
 * Must be used within a ReactFlow component (requires useReactFlow context).
 */
export function useNodeFocus({
  focusNodeId,
  onFocusApplied,
  duration = 500,
  padding = 0.5,
}: UseNodeFocusOptions): void {
  const { fitView, getNode } = useReactFlow();
  const prevFocusRef = useRef<string | undefined>(undefined);

  useEffect(() => {
    if (focusNodeId && focusNodeId !== prevFocusRef.current) {
      const timer = setTimeout(() => {
        const node = getNode(focusNodeId);
        if (node) {
          fitView({
            nodes: [{ id: focusNodeId }],
            duration,
            padding,
          });
        }
        onFocusApplied?.();
      }, NODE_FOCUS_DELAY_MS);
      prevFocusRef.current = focusNodeId;
      return () => clearTimeout(timer);
    } else if (!focusNodeId) {
      prevFocusRef.current = undefined;
    }
  }, [focusNodeId, fitView, getNode, onFocusApplied, duration, padding]);
}
