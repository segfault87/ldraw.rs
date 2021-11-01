precision mediump float;

in vec4 vColor;
in float discardFlag;

out vec4 fragColor;

void main(void) {
     if (discardFlag > 0.5f) {
          discard;
     }

     fragColor = vColor;
}
