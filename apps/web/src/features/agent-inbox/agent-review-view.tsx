import { SplitHeaderLeft } from '@components/app/split-layout/components/SplitHeader';
import { formatRelativeTimestamp } from '@entity/utils/timestamp';
import { createSignal } from 'solid-js';
import { AgentReviewList } from './agent-review-list';
import {
  useAgentInboxQuery,
  useClaimEscalationMutation,
  useDecideApprovalMutation,
  useDecideSkillProposalMutation,
  useResolveEscalationMutation,
} from './use-agent-inbox';

/**
 * Agent Review: HITL escalations, approvals, and skill proposals.
 *
 * Split URL sync stores components as `/component/<id>`, so this view restores
 * at `/component/agent-review`. `/agent-review` is registered like `/activity`
 * as a named layout path. `/agents/review` is not used — `/agents` is the
 * Agents list, and a two-segment path would decode as a block pair.
 */
export function AgentReviewView() {
  const inbox = useAgentInboxQuery();
  const claim = useClaimEscalationMutation();
  const resolve = useResolveEscalationMutation();
  const decideApproval = useDecideApprovalMutation();
  const decideProposal = useDecideSkillProposalMutation();

  const [selectedId, setSelectedId] = createSignal<string>();
  const [note, setNote] = createSignal('');
  const [resolution, setResolution] = createSignal('');
  const [actionError, setActionError] = createSignal<string>();

  const items = () => inbox.data?.items ?? [];
  const pending = () =>
    claim.isPending ||
    resolve.isPending ||
    decideApproval.isPending ||
    decideProposal.isPending;

  const select = (id: string) => {
    setSelectedId((current) => (current === id ? undefined : id));
    setNote('');
    setResolution('');
    setActionError(undefined);
  };

  const run = (action: () => Promise<unknown>) => {
    setActionError(undefined);
    void action().catch((error: unknown) => {
      setActionError(
        error instanceof Error
          ? error.message
          : 'That action could not be completed.'
      );
    });
  };

  return (
    <div class="@container/u-list flex size-full flex-col">
      <SplitHeaderLeft>
        <span class="font-semibold text-sm">Agent Review</span>
      </SplitHeaderLeft>
      <div class="min-h-0 flex-1 overflow-y-auto py-1">
        <AgentReviewList
          items={items()}
          isLoading={inbox.isLoading}
          isError={inbox.isError}
          selectedId={selectedId()}
          note={note()}
          resolution={resolution()}
          pending={pending()}
          actionError={actionError()}
          formatCreated={(iso) =>
            formatRelativeTimestamp(new Date(iso), { condensed: true })
          }
          onSelect={select}
          onNoteChange={setNote}
          onResolutionChange={setResolution}
          onClaim={(id) => run(() => claim.mutateAsync(id))}
          onResolve={(id) =>
            run(() =>
              resolve.mutateAsync({ id, resolution: resolution().trim() })
            )
          }
          onDecideApproval={(id, approved) =>
            run(() =>
              decideApproval.mutateAsync({
                id,
                approved,
                note: note().trim() || undefined,
              })
            )
          }
          onDecideSkillProposal={(id, approved) =>
            run(() =>
              decideProposal.mutateAsync({
                id,
                approved,
                note: note().trim() || undefined,
              })
            )
          }
        />
      </div>
    </div>
  );
}
