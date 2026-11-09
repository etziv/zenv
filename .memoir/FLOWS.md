# FLOWS (how the system works NOW; one entry per flow, edited in place, max 25 lines each)
<!-- Write a flow when you build or change one; the body is always the current behaviour, History says how it got
here. Name the code of every step in backticks (`Class.method`, `Controller@method`): `memory.mjs flow` checks each
one against the code graph and git, and flags the flow when that code changed after its newest History date.
A step that starts another flow links it ([F-002](#f-002-card-payment)): that flow keeps the step current.

## F-001 Checkout
Trigger: the customer presses Pay on the cart screen.
1. `OrderController.create` reads the cart: `CartService.total`.
2. `CouponService.apply` lowers the total before the charge (D-004).
3. `PaymentGateway.charge` takes the payment ([F-002](#f-002-card-payment)).
4. `OrderRepository.save` stores the paid order; `Mailer.sendReceipt` e-mails the receipt.
Rules: no unpaid order is ever saved (D-002).
History:
- 2026-01-01 task 1: created
- 2026-01-09 task 4: coupon before the charge
-->
