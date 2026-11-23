// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'package:core/auth_pay.dart';
import 'package:core/core.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:ui/ui.dart';

/// Bank details for a deposit that has not arrived yet. The booking confirms when it does.
class VirtualAccountScreen extends StatelessWidget {
  const VirtualAccountScreen({super.key, required this.account});

  /// Shown as the server sent it, amount included: the deposit must match it to the won.
  final VirtualAccount account;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final due = account.due;
    final amount = account.amount;
    return Scaffold(
      appBar: AppBar(title: KeepAllText(t.apVaTitle)),
      body: ListView(
        padding: const EdgeInsets.all(Space.gutter),
        children: [
          Tag(
            label: t.apVaWaiting,
            hue: StatusHues.assigned,
            icon: Icons.hourglass_top_rounded,
          ),
          const SizedBox(height: Space.sm),
          KeepAllText(t.apVaTitle, style: context.text.headlineSmall),
          const SizedBox(height: Space.xs),
          KeepAllText(
            t.apVaBody,
            style: context.text.bodyMedium?.copyWith(
              color: context.colors.inkSoft,
            ),
          ),
          const SizedBox(height: Space.lg),
          AppCard(
            child: Column(
              children: [
                _Row(label: t.apVaBank, value: t.bankName(account.bank)),
                _Row(
                  label: t.apVaAccount,
                  value: account.accountNumber,
                  emphasis: true,
                  action: IconButton(
                    tooltip: t.apVaCopy,
                    icon: const Icon(Icons.copy_rounded),
                    onPressed: () async {
                      await Clipboard.setData(
                        ClipboardData(text: account.accountNumber),
                      );
                      if (context.mounted) showToast(context, t.apVaCopied);
                    },
                  ),
                ),
                _Row(label: t.apVaHolder, value: account.holder),
                if (amount != null)
                  _Row(
                    label: t.apVaAmount,
                    value: context.won(amount),
                    emphasis: true,
                  ),
              ],
            ),
          ),
          if (due != null) ...[
            const SizedBox(height: Space.sm),
            KeepAllText(
              t.apVaDue(dateTimeLabel(due, context.lang)),
              style: context.text.titleSmall,
              textAlign: TextAlign.center,
            ),
          ],
        ],
      ),
    );
  }
}

class _Row extends StatelessWidget {
  const _Row({
    required this.label,
    required this.value,
    this.emphasis = false,
    this.action,
  });
  final String label;
  final String value;
  final bool emphasis;
  final Widget? action;

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.symmetric(vertical: Space.xxs),
    child: Row(
      children: [
        KeepAllText(
          label,
          style: context.text.bodyMedium?.copyWith(
            color: context.colors.inkSoft,
          ),
        ),
        const SizedBox(width: Space.md),
        Expanded(
          child: KeepAllText(
            value,
            textAlign: TextAlign.end,
            style:
                (emphasis ? context.text.titleMedium : context.text.bodyMedium)
                    ?.copyWith(fontWeight: FontWeight.w700),
          ),
        ),
        ?action,
      ],
    ),
  );
}
