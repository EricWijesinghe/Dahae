// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'package:core/core.dart';
import 'package:flutter/material.dart';
import 'package:ui/ui.dart';

import '../state.dart';
import '../widgets/owner_forms.dart';
import '../widgets/owner_totp_dialog.dart';
import 'confirm_action.dart';
import '../widgets/inline_button.dart';

/// 내 계정: the owner's own security settings, plus claiming or handing over the owner role.
/// Every owner-only change asks for a fresh authenticator code first.
class OwnerAccountScreen extends StatelessWidget {
  const OwnerAccountScreen({super.key});

  @override
  Widget build(BuildContext context) => AsyncView<Json>(
    load: () async => (await context.api.get('/admin/me/account')) as Json,
    builder: (context, me, reload) => ListView(
      padding: const EdgeInsets.all(Space.lg),
      children: me['is_owner'] == true
          ? _ownerCards(context, me, reload)
          : _adminCards(context, me, reload),
    ),
  );
}

List<Widget> _adminCards(
  BuildContext context,
  Json me,
  Future<void> Function() reload,
) {
  final t = context.t;
  final ready =
      me['two_factor_enrolled'] == true && me['recovery_confirmed'] == true;
  return [
    _Card(
      icon: Icons.shield_outlined,
      title: t.oaTitle,
      body: t.oaNotOwner,
      action: null,
    ),
    if (me['owner_exists'] != true)
      _Card(
        icon: Icons.verified_user_rounded,
        title: t.oaClaimTitle,
        body: ready ? t.oaClaimBody : t.oaTwoFactorHint,
        action: ready
            ? FilledButton.tonal(
                style: compactFilled,
                onPressed: () => _claim(context, reload),
                child: KeepAllText(t.oaClaim),
              )
            : null,
      ),
  ];
}

List<Widget> _ownerCards(
  BuildContext context,
  Json me,
  Future<void> Function() reload,
) {
  final t = context.t;
  final ready =
      me['two_factor_enrolled'] == true && me['recovery_confirmed'] == true;
  final phone = me['alert_phone_masked'] as String?;
  return [
    _Card(
      icon: Icons.workspace_premium_rounded,
      title: '${t.oaTitle} · ${t.oaOwnerBadge}',
      body: ready ? null : t.oaTwoFactorHint,
      action: null,
    ),
    if (!ready &&
        me['recovery_confirmed'] != true &&
        me['two_factor_enrolled'] == true)
      _Card(
        icon: Icons.checklist_rounded,
        title: t.oaConfirmCodes,
        body: null,
        action: FilledButton.tonal(
          style: compactFilled,
          onPressed: () => _confirmCodes(context, reload),
          child: KeepAllText(t.oaConfirmCodes),
        ),
      ),
    _Card(
      icon: Icons.sms_rounded,
      title: t.oaAlertPhone,
      body: phone == null ? t.oaAlertPhoneNone : '$phone · ${t.oaPhoneNote}',
      action: FilledButton.tonal(
        style: compactFilled,
        onPressed: () => _changePhone(context, reload),
        child: KeepAllText(t.oaChangePhone),
      ),
    ),
    _Card(
      icon: Icons.lock_reset_rounded,
      title: t.oaPassword,
      body: t.oaPasswordNote,
      action: FilledButton.tonal(
        style: compactFilled,
        onPressed: () => _changePassword(context),
        child: KeepAllText(t.oaPassword),
      ),
    ),
    _Card(
      icon: Icons.key_rounded,
      title: t.oaRecoveryCodes,
      body: t.oaRecoveryBody,
      action: FilledButton.tonal(
        style: compactFilled,
        onPressed: () => _reissueCodes(context, reload),
        child: KeepAllText(t.oaRecoveryCodes),
      ),
    ),
    _Card(
      icon: Icons.devices_rounded,
      title: t.oaSignOutAll,
      body: '${t.oaSessions}: ${me['active_sessions']}\n${t.oaSignOutAllBody}',
      action: FilledButton.tonal(
        style: compactFilled,
        onPressed: () => _signOutAll(context),
        child: KeepAllText(t.oaSignOutAll),
      ),
    ),
    _Card(
      icon: Icons.swap_horiz_rounded,
      title: t.oaTransfer,
      body: t.oaTransferBody,
      action: FilledButton.tonal(
        style: compactFilled,
        onPressed: () => _transfer(context, me, reload),
        child: KeepAllText(t.oaTransfer),
      ),
    ),
  ];
}

/// One setting: icon, title and note, with its button under the note (beside it, a phone-width tile
/// left the title too narrow to read).
class _Card extends StatelessWidget {
  const _Card({
    required this.icon,
    required this.title,
    required this.body,
    required this.action,
  });
  final IconData icon;
  final String title;
  final String? body;
  final Widget? action;

  @override
  Widget build(BuildContext context) => Card(
    elevation: 0,
    child: ListTile(
      leading: Icon(icon),
      title: KeepAllText(title, style: context.text.titleSmall),
      subtitle: action == null
          ? (body == null ? null : KeepAllText(body!))
          : Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                if (body != null) KeepAllText(body!),
                const SizedBox(height: Space.sm),
                action!,
              ],
            ),
      // Icon at the top of every card with a note, so the cards line up (a title alone stays centred beside it);
      // three-line height only for a button or a two-line note.
      titleAlignment: body == null && action == null
          ? ListTileTitleAlignment.center
          : ListTileTitleAlignment.top,
      isThreeLine: action != null || (body?.contains('\n') ?? false),
    ),
  );
}

Future<void> _claim(
  BuildContext context,
  Future<void> Function() reload,
) async {
  final t = context.t;
  if (!await confirmAction(context, t.oaClaim) || !context.mounted) return;
  final r = await runOwnerAction(
    context,
    (h) => context.api.post('/admin/owner/claim', null, null, h),
    done: t.oaClaimDone,
  );
  if (r != null) await reload();
}

Future<void> _changePhone(
  BuildContext context,
  Future<void> Function() reload,
) async {
  final t = context.t;
  final v = await askFields(
    context,
    title: t.oaAlertPhone,
    body: t.oaPhoneNote,
    confirm: t.oaChangePhone,
    fields: [FieldSpec(t.oaPhoneHint, phone: true)],
  );
  if (v == null || !context.mounted) return;
  final r = await runOwnerAction(
    context,
    (h) => context.api.put('/admin/me/alert-phone', {'phone': v.first}, h),
    done: t.oaPhoneSaved,
  );
  if (r != null) await reload();
}

Future<void> _changePassword(BuildContext context) async {
  final t = context.t;
  final v = await askFields(
    context,
    title: t.oaPassword,
    body: t.oaPasswordNote,
    confirm: t.oaPassword,
    fields: [
      FieldSpec(t.oaCurrentPassword, obscure: true),
      FieldSpec(t.oaNewPassword, obscure: true),
    ],
  );
  if (v == null || !context.mounted) return;
  final r = await runOwnerAction(
    context,
    (h) => context.api.post(
      '/admin/me/password',
      {'current_password': v[0], 'new_password': v[1]},
      null,
      h,
    ),
    done: t.oaPasswordChanged,
  );
  if (r != null && context.mounted) await context.console.session.logout();
}

Future<void> _signOutAll(BuildContext context) async {
  final t = context.t;
  if (!await confirmAction(context, t.oaSignOutAll) || !context.mounted) return;
  final r = await runOwnerAction(
    context,
    (h) => context.api.post('/admin/me/sign-out-all', null, null, h),
    done: t.oaSignInAgain,
  );
  if (r != null && context.mounted) await context.console.session.logout();
}

Future<void> _reissueCodes(
  BuildContext context,
  Future<void> Function() reload,
) async {
  final t = context.t;
  if (!await confirmAction(context, t.oaRecoveryCodes) || !context.mounted) {
    return;
  }
  final r = await runOwnerAction(
    context,
    (h) => context.api.post('/admin/me/recovery-codes', null, null, h),
    done: t.oaRecoveryShown,
  );
  if (r == null || !context.mounted) return;
  final codes = [
    for (final c in (r as Json)['recovery_codes'] as List) c as String,
  ];
  if (await showRecoveryCodes(context, codes) && context.mounted) {
    await _confirmCodes(context, reload);
  }
  await reload();
}

Future<void> _confirmCodes(
  BuildContext context,
  Future<void> Function() reload,
) async {
  final t = context.t;
  final code = await askRecoveryCode(context);
  if (code == null || !context.mounted) return;
  try {
    await context.api.post('/admin/me/recovery-codes/confirm', {'code': code});
    if (context.mounted) showToast(context, t.oaCodesConfirmed);
    await reload();
  } catch (e) {
    if (context.mounted) showToast(context, t.error(e));
  }
}

Future<void> _transfer(
  BuildContext context,
  Json me,
  Future<void> Function() reload,
) async {
  final t = context.t;
  final staff = (await context.api.get('/admin/staff') as List).cast<Json>();
  final candidates = [
    for (final s in staff)
      if (s['role'] == 'admin' && s['active'] == true && s['id'] != me['id']) s,
  ];
  if (!context.mounted) return;
  if (candidates.isEmpty) {
    showToast(context, t.oaTransferNone);
    return;
  }
  final pick = await askTransferTarget(context, candidates);
  if (pick == null || !context.mounted) return;
  final v = await askFields(
    context,
    title: t.oaTransferTarget,
    body: t.oaTransferBody,
    confirm: t.oaTransfer,
    fields: [FieldSpec(t.oaTransferCode, digits: true)],
  );
  if (v == null || !context.mounted) return;
  final r = await runOwnerAction(
    context,
    (h) => context.api.post(
      '/admin/owner/transfer',
      {'to': pick['id'], 'code': v.first},
      null,
      h,
    ),
    done: t.oaTransferDone,
  );
  if (r != null) await reload();
}
