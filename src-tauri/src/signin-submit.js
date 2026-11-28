(() => {
  if (location.origin !== 'https://www.natpierce.cn' || location.pathname !== '/pc/sign/index.html' || window.top !== window) return
  const button = document.querySelector('#qiandao')
  if (button instanceof HTMLElement && button.getClientRects().length && !button.matches(':disabled, [aria-disabled="true"]')) {
    button.click()
  }
})()
