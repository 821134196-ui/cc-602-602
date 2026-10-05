export const STATUS_TEXT = {
  uploaded: '待导师确认',
  advisor_confirmed: '待秘书匿名审查',
  frozen: '已冻结待分配',
  in_review: '评阅中',
  released: '意见已开放'
}

export const REC_TEXT = {
  pass: '通过',
  minor_revision: '小修后通过',
  major_revision: '大修后重审',
  reject: '不通过'
}

export function fmtTime(s) {
  return s ? s.replace('T', ' ').slice(0, 16) : '-'
}
