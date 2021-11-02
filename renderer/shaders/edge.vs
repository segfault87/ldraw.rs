precision mediump float;

in vec3 position;
in vec3 color;

#ifdef USE_INSTANCING
    in vec4 instancedColor;
    in vec4 instancedEdgeColor;
    in mat4 instancedModelMatrix;
#else
    uniform vec4 defaultColor;
    uniform vec4 edgeColor;
#endif

uniform mat4 modelMatrix;
uniform mat4 projection;
uniform mat4 modelView;

out vec4 vColor;

void main(void) {
    vec4 mvPosition = vec4(position, 1.0);
    #ifdef USE_INSTANCING
        mvPosition = instancedModelMatrix * mvPosition;
    #endif
    mvPosition = modelView * mvPosition;
    gl_Position = projection * mvPosition;
    #ifdef USE_INSTANCING
        if (color.x < -1.0) {
            vColor = instancedEdgeColor;
        } else if (color.x < 0.0) {
            vColor = instancedColor;
        } else {
            vColor = vec4(color, 1.0);
        }
    #else
        if (color.x < -1.0) {
            vColor = edgeColor;
        } else if (color.x < 0.0) {
            vColor = defaultColor;
        } else {
            vColor = vec4(color, 1.0);
        }
    #endif
}
