// Isolated Linux fixture: two readable FDs prove FLTK wait may return > 1.
// Uses the same built native libraries as files; no window or clipboard access.
#include <FL/Fl.H>
#include <cerrno>
#include <initializer_list>
#include <cstdio>
#include <unistd.h>
extern "C" double Fl_wait_for(double);
static int callbacks = 0;
static void ready(int fd, void *) {
  char byte;
  if (::read(fd, &byte, 1) == 1) ++callbacks;
}
int main() {
  int a[2], b[2];
  if (::pipe(a) != 0) return 2;
  if (::pipe(b) != 0) { ::close(a[0]); ::close(a[1]); return 2; }
  Fl::lock();
  Fl::add_fd(a[0], FL_READ, ready);
  Fl::add_fd(b[0], FL_READ, ready);
  bool wrote = ::write(a[1], "a", 1) == 1 && ::write(b[1], "b", 1) == 1;
  errno = 0;
  double result = wrote ? Fl_wait_for(0.1) : -1;
  int saved_errno = errno;
  Fl::remove_fd(a[0]); Fl::remove_fd(b[0]);
  for (int fd : {a[0], a[1], b[0], b[1]}) ::close(fd);
  std::printf("{\"raw_wait\":%.0f,\"callbacks\":%d,\"errno\":%d,\"rust_1_5_23_would_reject\":%s}\n",
      result, callbacks, saved_errno, result > 1 ? "true" : "false");
  return wrote && result >= 2 && callbacks == 2 ? 0 : 1;
}
