import 'package:core/core.dart';
import 'package:flutter/material.dart';
import 'package:ui/ui.dart';

import '../state.dart';
import '../widgets/owner_totp_dialog.dart';
import 'stepup.dart';

/// The owner's way out of a Toss send that ended without an answer while Toss cannot be asked: they check
/// the Toss dashboard and state the money did not leave. Explains what to look for, then asks for the
/// owner's code. Resolves to true once the server marked the payout failed.
Future<bool> confirmPayoutNotSent(BuildContext context, Json payout) async {
  final ok = await showDialog<bool>(
    context: context,
    builder: (_) => PayoutNotSentDialog(payout: payout),
  );
  if (ok != true || !context.mounted) return false;
  final code = await askOwnerTotp(context);
  if (code == null || !context.mounted) return false;
  try {
    await withStepUp(
      context,
      () => context.api.post(
        '/admin/settlements/payouts/${payout['id']}/not-sent',
        {},
        null,
        ownerHeaders(code),
      ),
    );
    if (context.mounted) showToast(context, context.t.poNotSentDone);
    return true;
  } catch (e) {
    if (context.mounted) showToast(context, context.t.error(e));
    return false;
  }
}

/// What to check in the Toss dashboard. The confirm stays off until the owner ticks that they checked.
class PayoutNotSentDialog extends StatefulWidget {
  const PayoutNotSentDialog({super.key, required this.payout});

  final Json payout;

  @override
  State<PayoutNotSentDialog> createState() => _PayoutNotSentDialogState();
}

class _PayoutNotSentDialogState extends State<PayoutNotSentDialog> {
  bool _checked = false;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final p = widget.payout;
    final amount = ((p['net'] as num?) ?? (p['amount'] as num)).toInt();
    return AlertDialog(
      title: Text(t.poNotSentTitle),
      content: SingleChildScrollView(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(t.poNotSentBody('${p['pro_name']}', context.won(amount))),
            const SizedBox(height: Space.sm),
            Text(
              t.poNotSentWarning,
              style: context.text.bodyMedium?.copyWith(
                color: context.colors.danger,
                fontWeight: FontWeight.w600,
              ),
            ),
            CheckboxListTile(
              contentPadding: EdgeInsets.zero,
              controlAffinity: ListTileControlAffinity.leading,
              value: _checked,
              onChanged: (v) => setState(() => _checked = v ?? false),
              title: Text(t.poNotSentChecked),
            ),
          ],
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(context).pop(false),
          child: Text(t.close),
        ),
        FilledButton(
          onPressed: _checked ? () => Navigator.of(context).pop(true) : null,
          child: Text(t.poNotSentConfirm),
        ),
      ],
    );
  }
}
