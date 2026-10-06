-- Credit Sales → Recall: goods on a credit sale come back to the branch they were sold from, through the same
-- return machinery (stock, ledger, loyalty, credit balance), recorded as a 'recall' with the balance before/after.
ALTER TABLE sale_returns DROP CONSTRAINT sale_returns_kind_check;
ALTER TABLE sale_returns ADD CONSTRAINT sale_returns_kind_check CHECK (kind IN ('return','cancellation','recall'));
-- Credit balance before and after the return (credit sales), and any payments beyond the revised amount owed that
-- the customer is due back: refunded at once (refund_method) or left for follow-up (customer_credit).
ALTER TABLE sale_returns ADD COLUMN balance_before numeric(14,2);
ALTER TABLE sale_returns ADD COLUMN balance_after numeric(14,2);
ALTER TABLE sale_returns ADD COLUMN customer_credit numeric(14,2) NOT NULL DEFAULT 0 CHECK (customer_credit >= 0);

-- Recall state shown on the credit sale; collection status (outstanding/paid…) keeps working for what is still owed.
ALTER TABLE credit_sales ADD COLUMN recall_state text CHECK (recall_state IN ('partially_recalled','recalled'));
ALTER TABLE credit_sales DROP CONSTRAINT credit_sales_status_check;
ALTER TABLE credit_sales ADD CONSTRAINT credit_sales_status_check
    CHECK (status IN ('outstanding','partially_paid','paid','written_off','cancelled','recalled'));

-- Who may recall: roles that can already process returns and see credit.
UPDATE roles SET permissions = array_append(permissions, 'credit.recall')
 WHERE NOT ('*' = ANY(permissions)) AND NOT ('credit.recall' = ANY(permissions))
   AND 'sales.return' = ANY(permissions) AND 'credit.view' = ANY(permissions);
