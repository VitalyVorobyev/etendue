/**
 * Keeps an error thrown while rendering (a kernel call in a view, say) inside the part of the
 * studio it happened in: the part shows what went wrong and a way back, and the rest of the
 * studio keeps working.
 */

import { Button, Callout } from "@vitavision/ui";
import { Component, type ErrorInfo, type ReactNode } from "react";

import { describeFailure } from "../kernel/etendue";

interface Props {
  /** What stopped, for the title: "This view", "The studio". */
  area: string;
  /** The part starts again when this changes (the workspace, the document). */
  resetKey?: unknown;
  /** The way back; by default "Try again", which renders the part again. */
  recover?: { label: string; run: () => void };
  children: ReactNode;
}

interface State {
  error: unknown;
  resetKey: unknown;
}

export class ErrorBoundary extends Component<Props, State> {
  override state: State = { error: null, resetKey: this.props.resetKey };

  static getDerivedStateFromError(error: unknown): Partial<State> {
    return { error };
  }

  static getDerivedStateFromProps(props: Props, state: State): Partial<State> | null {
    return props.resetKey === state.resetKey ? null : { error: null, resetKey: props.resetKey };
  }

  override componentDidCatch(error: unknown, info: ErrorInfo): void {
    console.error(error, info.componentStack);
  }

  override render(): ReactNode {
    const { error } = this.state;
    if (error === null) return this.props.children;
    const { area, recover } = this.props;
    const { message } = describeFailure(error);
    return (
      <div className="grid h-full place-items-center p-6" data-testid="error-boundary">
        <Callout
          tone="error"
          title={`${area} stopped`}
          actions={
            <Button size="sm" onClick={recover ? recover.run : () => this.setState({ error: null })}>
              {recover ? recover.label : "Try again"}
            </Button>
          }
        >
          {message}
        </Callout>
      </div>
    );
  }
}
