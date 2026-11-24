// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'package:core/core.dart';
import 'package:flutter/material.dart';
import 'package:ui/ui.dart';

import '../state.dart';
import '../widgets/owner_totp_dialog.dart';
import 'confirm_action.dart';
import 'staff_code_dialogs.dart';

/// Owner-initiated password reset by one-time SMS code. The code is texted to the staff member, or shown once
/// so the owner can hand it over. It works once, for 24 hours.
/// With `invite`, the staff member has not set a password yet, so the code activates the account instead.
Future<void> issueStaffPasswordReset(
  BuildContext context,
  Json staff, {
  bool invite = false,
}) async {
  final t = context.t;
  final prompt = invite ? t.cStaffInviteResend : t.cStaffReset;
  if (!await confirmAction(context, prompt) || !context.mounted) return;
  // An admin account (the owner's included) gets a code only with the owner's code; a consultant does not.
  final needsOwner = staff['is_owner'] == true || staff['role'] == 'admin';
  final code = needsOwner ? await askOwnerTotp(context) : '';
  if (code == null || !context.mounted) return;
  final delivery = await pickCodeDelivery(
    context,
    needsPhone: staff['has_phone'] != true,
  );
  if (delivery == null || !context.mounted) return;
  try {
    final r =
        await context.api.post(
              '/admin/staff/${staff['id']}/password-reset',
              {'channel': delivery.channel, 'phone': delivery.phone},
              null,
              needsOwner ? ownerHeaders(code) : null,
            )
            as Json;
    if (!context.mounted) return;
    await showSentCode(context, r, t.cStaffResetTitle);
  } catch (e) {
    if (context.mounted) showToast(context, t.error(e));
  }
}
