import { defineRailway, github, preserve, project, service } from "railway/iac";

// Roadmap 94: replaces railway.json (Config as Code), which Railway stops reading on 2026-12-01.
//
// In this format anything the file leaves out is removed from Railway, so it declares everything the service has:
// - the GitHub source (main, deploys only after CI passes),
// - every variable as preserve() — "keep the value already set in Railway". Values never live in this file.
//   A variable added on Railway later must be added here too (as preserve()), or the next apply deletes it.
// - the health check (/healthz, 120 s) that railway.json set.
// Railway builds with the root Dockerfile (detected automatically; the format has no builder setting) and restarts
// on failure by default. Always run `railway config plan` and read it before `railway config apply`.
export const partial = "sshop";

export default defineRailway(() => {
  const sshop = service("sshop", {
    source: github("Jemwanzs/S-Shop", { branch: "main", checkSuites: true }),
    healthcheck: "/healthz",
    healthcheckTimeout: 120,
    env: {
      BOOTSTRAP_ADMIN_EMAIL: preserve(),
      BOOTSTRAP_ADMIN_NAME: preserve(),
      BOOTSTRAP_BUSINESS_NAME: preserve(),
      DATABASE_URL: preserve(),
      DATA_ENCRYPTION_KEY: preserve(),
      JWT_SECRET: preserve(),
      MAIL_FROM: preserve(),
      MPESA_CALLBACK_URL: preserve(),
      MPESA_CONSUMER_KEY: preserve(),
      MPESA_CONSUMER_SECRET: preserve(),
      MPESA_ENVIRONMENT: preserve(),
      MPESA_PROD_CALLBACK_URL: preserve(),
      MPESA_PROD_CONSUMER_KEY: preserve(),
      MPESA_PROD_CONSUMER_SECRET: preserve(),
      MPESA_PROD_SHORTCODE: preserve(),
      MPESA_PROD_STK_PASSKEY: preserve(),
      MPESA_PROD_TILL_NUMBER: preserve(),
      MPESA_SHORTCODE: preserve(),
      MPESA_STK_PASSKEY: preserve(),
      PAYSTACK_SECRET_KEY: preserve(),
      PLATFORM_ADMIN_EMAILS: preserve(),
      PUBLIC_URL: preserve(),
      RAILWAY_API_TOKEN: preserve(),
      RESEND_API_KEY: preserve(),
      RUST_LOG: preserve(),
    },
  });
  return project("S'Shop", {
    resources: [sshop],
  });
});
