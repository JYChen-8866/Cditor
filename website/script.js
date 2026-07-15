const reveals = document.querySelectorAll('.reveal');
const observer = new IntersectionObserver((entries) => {
  entries.forEach((entry) => {
    if (entry.isIntersecting) {
      entry.target.classList.add('visible');
      observer.unobserve(entry.target);
    }
  });
}, { threshold: 0.12 });

reveals.forEach((item) => observer.observe(item));

const copyButton = document.querySelector('#copyButton');
copyButton.addEventListener('click', async () => {
  const command = document.querySelector('#command').textContent;
  try {
    await navigator.clipboard.writeText(command);
    copyButton.textContent = '已复制';
    window.setTimeout(() => { copyButton.textContent = '复制'; }, 1600);
  } catch {
    copyButton.textContent = '请手动复制';
  }
});
