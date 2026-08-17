/**
 * Mount Macro-governed skills into a Flue agent via `defineSkill` / `useSkill`.
 *
 * Catalog fetch is kicked off when the runtime is built and cached on the
 * runtime. Each render mounts whatever is currently cached; the next turn
 * picks up newly arrived skills. Injection is recorded once per skill
 * version on this conversation's ledger (`skill_injected`).
 */

import { defineSkill, useSkill } from '@flue/runtime';
import type { MacroSessionContext } from '../sessions/macro-session.ts';
import type { SkillCatalogEntry } from './client.ts';

const SKILL_NAME = /^[a-z0-9]+(?:-[a-z0-9]+)*$/;

/** Whether a catalog slug is a valid Flue skill name. */
export function isFlueSkillName(slug: string): boolean {
  return slug.length > 0 && slug.length <= 64 && SKILL_NAME.test(slug);
}

/**
 * Mount cached catalog entries and emit `skill_injected` the first time
 * each skill version is seen on this conversation.
 */
export function mountGovernedSkills(
  session: MacroSessionContext,
  skills: readonly SkillCatalogEntry[],
): void {
  for (const skill of skills) {
    if (!isFlueSkillName(skill.slug)) {
      continue;
    }
    useSkill(
      defineSkill({
        name: skill.slug,
        description: skill.description,
        instructions: skill.body,
      }),
    );
    session.recordSkillInjection(skill);
  }
}
