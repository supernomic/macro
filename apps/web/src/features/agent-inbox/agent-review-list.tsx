import type { AgentInboxItem } from '@service-cognition/client';
import { For, Show } from 'solid-js';
import { assignmentLabel, kindLabel, listFallbackMessage } from './labels';

const ROW_BODY_CLASS =
  'flex min-h-10 min-w-0 flex-1 items-center gap-1.5 rounded-lg px-2 py-0.5 hover:bg-hover/30';

const ACTION_BUTTON_CLASS =
  'h-6 px-2 text-xs font-medium rounded-sm border border-transparent disabled:opacity-30';

/** Presentational Agent Review list — empty, loading, error, and rows. */
export function AgentReviewList(props: {
  items: AgentInboxItem[];
  isLoading: boolean;
  isError: boolean;
  selectedId?: string;
  note?: string;
  resolution?: string;
  pending?: boolean;
  actionError?: string;
  formatCreated?: (iso: string) => string;
  onSelect?: (id: string) => void;
  onNoteChange?: (value: string) => void;
  onResolutionChange?: (value: string) => void;
  onClaim?: (id: string) => void;
  onResolve?: (id: string) => void;
  onDecideApproval?: (id: string, approved: boolean) => void;
  onDecideSkillProposal?: (id: string, approved: boolean) => void;
}) {
  const fallback = () =>
    listFallbackMessage({
      itemCount: props.items.length,
      isLoading: props.isLoading,
      isError: props.isError,
    });
  const createdLabel = (iso: string) =>
    props.formatCreated ? props.formatCreated(iso) : iso;

  return (
    <Show
      when={fallback() === undefined}
      fallback={<p class="px-3 py-2 text-ink-muted text-sm">{fallback()}</p>}
    >
      <For each={props.items}>
        {(item) => (
          <InboxRow
            item={item}
            selected={props.selectedId === item.id}
            note={props.note ?? ''}
            resolution={props.resolution ?? ''}
            pending={props.pending ?? false}
            createdLabel={createdLabel(item.created_at)}
            actionError={
              props.selectedId === item.id ? props.actionError : undefined
            }
            onSelect={props.onSelect}
            onNoteChange={props.onNoteChange}
            onResolutionChange={props.onResolutionChange}
            onClaim={props.onClaim}
            onResolve={props.onResolve}
            onDecideApproval={props.onDecideApproval}
            onDecideSkillProposal={props.onDecideSkillProposal}
          />
        )}
      </For>
    </Show>
  );
}

function InboxRow(props: {
  item: AgentInboxItem;
  selected: boolean;
  note: string;
  resolution: string;
  pending: boolean;
  createdLabel: string;
  actionError: string | undefined;
  onSelect?: (id: string) => void;
  onNoteChange?: (value: string) => void;
  onResolutionChange?: (value: string) => void;
  onClaim?: (id: string) => void;
  onResolve?: (id: string) => void;
  onDecideApproval?: (id: string, approved: boolean) => void;
  onDecideSkillProposal?: (id: string, approved: boolean) => void;
}) {
  return (
    <div class="mx-1 px-2 text-sm">
      <button
        type="button"
        class={`${ROW_BODY_CLASS} w-full text-left`}
        onClick={() => props.onSelect?.(props.item.id)}
      >
        <span class="shrink-0 font-medium text-ink-muted">
          {kindLabel(props.item.kind)}
        </span>
        <span class="min-w-0 truncate font-medium">{props.item.title}</span>
        <span class="shrink-0 text-ink-muted">{props.item.status}</span>
        <span class="shrink-0 text-ink-extra-muted">
          {assignmentLabel(props.item)}
        </span>
        <span class="ml-auto shrink-0 text-right font-medium text-ink-extra-muted text-xs">
          {props.createdLabel}
        </span>
      </button>
      <Show when={props.selected}>
        <div class="flex flex-col gap-2 px-2 pb-3 pt-1">
          <Show when={props.item.kind === 'escalation'}>
            <EscalationActions
              item={props.item}
              resolution={props.resolution}
              pending={props.pending}
              onResolutionChange={props.onResolutionChange}
              onClaim={props.onClaim}
              onResolve={props.onResolve}
            />
          </Show>
          <Show when={props.item.kind === 'approval'}>
            <DecideActions
              pending={props.pending}
              note={props.note}
              onNoteChange={props.onNoteChange}
              onApprove={() => props.onDecideApproval?.(props.item.id, true)}
              onDeny={() => props.onDecideApproval?.(props.item.id, false)}
            />
          </Show>
          <Show when={props.item.kind === 'skill_proposal'}>
            <DecideActions
              pending={props.pending}
              note={props.note}
              onNoteChange={props.onNoteChange}
              onApprove={() =>
                props.onDecideSkillProposal?.(props.item.id, true)
              }
              onDeny={() => props.onDecideSkillProposal?.(props.item.id, false)}
            />
          </Show>
          <Show when={props.actionError}>
            <p class="text-failure text-xs">{props.actionError}</p>
          </Show>
        </div>
      </Show>
    </div>
  );
}

function EscalationActions(props: {
  item: AgentInboxItem;
  resolution: string;
  pending: boolean;
  onResolutionChange?: (value: string) => void;
  onClaim?: (id: string) => void;
  onResolve?: (id: string) => void;
}) {
  return (
    <>
      <Show when={!props.item.assigned_to_me}>
        <div>
          <button
            type="button"
            class={`${ACTION_BUTTON_CLASS} border-edge-muted text-ink-muted hover:bg-hover hover:text-ink`}
            disabled={props.pending}
            onClick={(event) => {
              event.stopPropagation();
              props.onClaim?.(props.item.id);
            }}
          >
            Claim
          </button>
        </div>
      </Show>
      <Show when={props.item.assigned_to_me}>
        <label class="flex flex-col gap-1 text-ink-muted text-xs">
          Resolution
          <textarea
            class="settings-input h-auto min-h-16 w-full resize-none px-3 py-2.5 leading-5 text-ink text-sm"
            value={props.resolution}
            disabled={props.pending}
            onInput={(event) =>
              props.onResolutionChange?.(event.currentTarget.value)
            }
          />
        </label>
        <div>
          <button
            type="button"
            class={`${ACTION_BUTTON_CLASS} bg-accent text-surface`}
            disabled={props.pending || props.resolution.trim().length === 0}
            onClick={(event) => {
              event.stopPropagation();
              props.onResolve?.(props.item.id);
            }}
          >
            Resolve
          </button>
        </div>
      </Show>
    </>
  );
}

function DecideActions(props: {
  pending: boolean;
  note: string;
  onNoteChange?: (value: string) => void;
  onApprove: () => void;
  onDeny: () => void;
}) {
  return (
    <>
      <label class="flex flex-col gap-1 text-ink-muted text-xs">
        Note
        <textarea
          class="settings-input h-auto min-h-16 w-full resize-none px-3 py-2.5 leading-5 text-ink text-sm"
          value={props.note}
          disabled={props.pending}
          onInput={(event) => props.onNoteChange?.(event.currentTarget.value)}
        />
      </label>
      <div class="flex gap-1.5">
        <button
          type="button"
          class={`${ACTION_BUTTON_CLASS} bg-accent text-surface`}
          disabled={props.pending}
          onClick={(event) => {
            event.stopPropagation();
            props.onApprove();
          }}
        >
          Approve
        </button>
        <button
          type="button"
          class={`${ACTION_BUTTON_CLASS} bg-failure/10 text-failure hover:bg-failure/25`}
          disabled={props.pending}
          onClick={(event) => {
            event.stopPropagation();
            props.onDeny();
          }}
        >
          Deny
        </button>
      </div>
    </>
  );
}
