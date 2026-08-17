import { throwOnErr } from '@core/util/result';
import { queryClient } from '@queries/client';
import { cognitionApiServiceClient } from '@service-cognition/client';
import { useMutation, useQuery } from '@tanstack/solid-query';

export const AGENT_INBOX_QUERY_KEY = ['agent-inbox', 'mine'] as const;

function invalidateInbox() {
  return queryClient.invalidateQueries({ queryKey: AGENT_INBOX_QUERY_KEY });
}

/** Fetches the current user's Agent Review inbox. */
export function useAgentInboxQuery() {
  return useQuery(() => ({
    queryKey: AGENT_INBOX_QUERY_KEY,
    queryFn: async () =>
      throwOnErr(async () => await cognitionApiServiceClient.getMyInbox()),
  }));
}

/** Claim a team-queue escalation, then refresh the inbox. */
export function useClaimEscalationMutation() {
  return useMutation(() => ({
    mutationFn: async (id: string) =>
      throwOnErr(
        async () => await cognitionApiServiceClient.claimEscalation({ id })
      ),
    onSuccess: () => invalidateInbox(),
  }));
}

/** Resolve a claimed escalation with an expert answer. */
export function useResolveEscalationMutation() {
  return useMutation(() => ({
    mutationFn: async (args: { id: string; resolution: string }) =>
      throwOnErr(
        async () => await cognitionApiServiceClient.resolveEscalation(args)
      ),
    onSuccess: () => invalidateInbox(),
  }));
}

/** Approve or deny a pending approval. No claim step. */
export function useDecideApprovalMutation() {
  return useMutation(() => ({
    mutationFn: async (args: {
      id: string;
      approved: boolean;
      note?: string;
    }) =>
      throwOnErr(
        async () => await cognitionApiServiceClient.decideApproval(args)
      ),
    onSuccess: () => invalidateInbox(),
  }));
}

/** Approve or reject a skill proposal. No claim step. */
export function useDecideSkillProposalMutation() {
  return useMutation(() => ({
    mutationFn: async (args: {
      id: string;
      approved: boolean;
      note?: string;
    }) =>
      throwOnErr(
        async () => await cognitionApiServiceClient.decideSkillProposal(args)
      ),
    onSuccess: () => invalidateInbox(),
  }));
}
