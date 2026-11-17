#include "../../../../Kirikiroid2/src/plugins/emoteplayer/emotegeometry.h"
#include <cstdio>
using namespace emoteplayer;
int main() {
  std::vector<emoteRender> c(4);
  c[0].type = 3;
  c[0].attachMat[0][0] = c[0].attachMat[1][1] = .05f;
  c[0].currCoordx = -2;
  c[0].currCoordy = 1.6f;
  c[1].type = 2;
  c[1].currAngle = 90;
  c[1].currZx = 1.25f;
  c[1].currZy = .75f;
  c[1].currSx = .2f;
  c[1].currSy = -.1f;
  c[1].currCoordx = -3;
  c[1].currCoordy = 2;
  c[1].originX = 100;
  c[1].originY = -200;
  c[1].width = 90;
  c[1].height = 80;
  c[1].currOx = 200;
  c[1].currOy = -300;
  c[2].type = 1;
  c[2].originX = 10;
  c[2].originY = 8;
  c[2].width = 20;
  c[2].height = 16;
  c[2].currOx = .75f;
  c[2].currOy = -.25f;
  c[2].currCoordx = 1.25f;
  c[2].currCoordy = -1.5f;
  c[2].currZx = 1.5f;
  c[2].currZy = .5f;
  c[2].currInheritMask = 0x1b0;
  for (int row = 0; row < 4; ++row)
    for (int col = 0; col < 4; ++col) {
      const int at = row * 8 + col * 2;
      c[2].controlPts[at] = col / 3.f + ((row == 1 || row == 2) ? .1f : 0.f);
      c[2].controlPts[at + 1] =
          row / 3.f + ((col == 1 || col == 2) ? .2f / 3.f : 0.f);
    }
  c[3].type = 2;
  c[3].originX = 4;
  c[3].originY = 3;
  c[3].width = 8;
  c[3].height = 6;
  c[3].currOx = 1;
  c[3].currOy = -.5f;
  c[3].currCoordx = 2;
  c[3].currCoordy = 1;
  c[3].currZx = 1.25f;
  c[3].currZy = .75f;
  for (unsigned mask :
       {0u, 0x10u, 0x20u, 0x40u, 0x80u, 0x100u, 0x1f0u, 0x20007fcu}) {
    c[3].currInheritMask = mask;
    for (const auto uv : {math::vec3(0, 0, 0), math::vec3(.375f, .625f, 0),
                          math::vec3(1, 1, 0)}) {
      float x, y;
      evaluateSurfaceChain(c, uv.x, uv.y, x, y);
      printf("(0x%x, %.3f, %.3f, [%.9f, %.9f]),\n", mask, uv.x, uv.y, x, -y);
    }
  }
}
