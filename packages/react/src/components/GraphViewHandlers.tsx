import { useEffect, useRef } from 'react';
import { useReactFlow } from '@xyflow/react';
import type { Viewport } from '@xyflow/react';

import { NODE_FOCUS_DELAY_MS } from '../constants';
import { useNodeFocus } from '../hooks/useNodeFocus';
import { useLineageStore } from '../store';

/**
 * Helper component to handle node focusing.
 * Must be rendered inside ReactFlow to access useReactFlow hook.
 */
export function NodeFocusHandler({
  focusNodeId,
  onFocusApplied,
}: {
  focusNodeId?: string;
  onFocusApplied?: () => void;
}): null {
  useNodeFocus({ focusNodeId, onFocusApplied });
  return null;
}

/**
 * Watches the store's `revealRequest` and drives both the graph's fitView
 * animation and the transient pulse class on the target node. A nonce is used
 * instead of a plain node id so re-revealing the same node restarts the
 * animation.
 */
export function RevealHandler({ applyPulse }: { applyPulse: (nodeId: string) => void }): null {
  const revealRequest = useLineageStore((store) => store.revealRequest);
  const clearRevealRequest = useLineageStore((store) => store.clearRevealRequest);
  const { fitView, getNode } = useReactFlow();
  const lastNonceRef = useRef<number | null>(null);

  useEffect(() => {
    if (!revealRequest) {
      lastNonceRef.current = null;
      return;
    }
    if (revealRequest.nonce === lastNonceRef.current) return;
    lastNonceRef.current = revealRequest.nonce;

    // ReactFlow needs a tick to render newly-selected nodes before we can
    // query their positions (same reason `useNodeFocus` uses NODE_FOCUS_DELAY_MS).
    const timer = setTimeout(() => {
      const node = getNode(revealRequest.nodeId);
      if (node) {
        fitView({ nodes: [{ id: revealRequest.nodeId }], duration: 500, padding: 0.5 });
        applyPulse(revealRequest.nodeId);
      }
      clearRevealRequest();
    }, NODE_FOCUS_DELAY_MS);

    return () => clearTimeout(timer);
  }, [revealRequest, fitView, getNode, applyPulse, clearRevealRequest]);

  return null;
}

/**
 * Helper component to trigger fitView when fitViewTrigger changes.
 * Must be rendered inside ReactFlow to access useReactFlow hook.
 */
export function FitViewHandler({ trigger }: { trigger?: number }): null {
  const { fitView } = useReactFlow();
  const lastTriggerRef = useRef(trigger);

  useEffect(() => {
    if (trigger !== undefined && trigger !== lastTriggerRef.current) {
      lastTriggerRef.current = trigger;
      // Small delay to ensure nodes are rendered
      setTimeout(() => {
        fitView({ padding: 0.2, duration: 200 });
      }, 50);
    }
  }, [trigger, fitView]);

  return null;
}

/**
 * Helper component to handle viewport changes and restoration.
 * Must be rendered inside ReactFlow to access useReactFlow hook.
 */
export function ViewportHandler({
  initialViewport,
  onViewportChange,
}: {
  initialViewport?: Viewport;
  onViewportChange?: (viewport: Viewport) => void;
}): null {
  const { setViewport, getViewport } = useReactFlow();
  const hasRestoredRef = useRef(false);
  const previousInitialViewportRef = useRef<Viewport | undefined>(initialViewport);
  const viewportChangeTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Cleanup timer on unmount
  useEffect(() => {
    return () => {
      if (viewportChangeTimerRef.current) {
        clearTimeout(viewportChangeTimerRef.current);
      }
    };
  }, []);

  // Reset restoration flag when initial viewport changes (e.g., project switch)
  useEffect(() => {
    if (previousInitialViewportRef.current !== initialViewport) {
      hasRestoredRef.current = false;
      previousInitialViewportRef.current = initialViewport;
    }
  }, [initialViewport]);

  // Restore initial viewport as needed
  useEffect(() => {
    if (initialViewport && !hasRestoredRef.current) {
      // Delay to ensure ReactFlow is ready
      const timer = setTimeout(() => {
        setViewport(initialViewport, { duration: 0 });
        hasRestoredRef.current = true;
      }, 100);
      return () => clearTimeout(timer);
    }
  }, [initialViewport, setViewport]);

  // Debounced viewport change callback
  useEffect(() => {
    if (!onViewportChange) return;

    const handleViewportChange = () => {
      if (viewportChangeTimerRef.current) {
        clearTimeout(viewportChangeTimerRef.current);
      }
      viewportChangeTimerRef.current = setTimeout(() => {
        const viewport = getViewport();
        onViewportChange(viewport);
      }, 300);
    };

    // Use MutationObserver on the viewport's style attribute rather than ReactFlow's
    // onMove/onViewportChange events. Those events fire at very high frequency during
    // pan/zoom operations which would cause excessive state updates and re-renders.
    // The MutationObserver approach with debouncing provides more reliable, batched updates.
    const container = document.querySelector('.react-flow__viewport');
    if (container) {
      const observer = new MutationObserver(handleViewportChange);
      observer.observe(container, { attributes: true, attributeFilter: ['style'] });
      return () => {
        observer.disconnect();
        if (viewportChangeTimerRef.current) {
          clearTimeout(viewportChangeTimerRef.current);
        }
      };
    }
  }, [onViewportChange, getViewport]);

  return null;
}
