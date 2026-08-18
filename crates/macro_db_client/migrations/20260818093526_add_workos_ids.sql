ALTER TABLE "Organization"
    ADD COLUMN workos_organization_id TEXT;

ALTER TABLE "User"
    ADD COLUMN workos_user_id TEXT;

CREATE UNIQUE INDEX "Organization_workos_organization_id_key"
    ON "Organization" (workos_organization_id)
    WHERE workos_organization_id IS NOT NULL;

CREATE UNIQUE INDEX "User_workos_user_id_key"
    ON "User" (workos_user_id)
    WHERE workos_user_id IS NOT NULL;
